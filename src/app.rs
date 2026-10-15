use crate::history::History;
use crate::runner;
use crate::session::{self, Change, Role, Session, SessionMeta};
use eframe::egui::{self, Color32, Id, Key, Modifiers, RichText};
use similar::{ChangeTag, TextDiff};
use std::sync::mpsc::Receiver;

const MAX_DIFF_LINES: usize = 200;

pub struct App {
    sessions: Vec<SessionMeta>,
    selected: Option<usize>,
    open: Option<Session>,
    error: Option<String>,
    input: String,
    history: History,
    running: Option<Receiver<Result<String, String>>>,
}

impl App {
    pub fn new() -> Self {
        Self {
            sessions: session::list(&session::projects_dir()),
            selected: None,
            open: None,
            error: None,
            input: String::new(),
            history: History::default(),
            running: None,
        }
    }

    fn select(&mut self, i: usize) {
        self.selected = Some(i);
        self.reload();
    }

    fn reload(&mut self) {
        let Some(meta) = self.selected.and_then(|i| self.sessions.get(i)) else { return };
        match session::load(&meta.path) {
            Ok(s) => {
                self.history = History::new(s.prompts.clone());
                self.open = Some(s);
                self.error = None;
            }
            Err(e) => self.error = Some(format!("could not read {}: {e}", meta.path.display())),
        }
    }

    fn refresh_list(&mut self) {
        let current = self.selected.map(|i| self.sessions[i].id.clone());
        self.sessions = session::list(&session::projects_dir());
        self.selected = current.and_then(|id| self.sessions.iter().position(|s| s.id == id));
        self.reload();
    }

    fn send(&mut self, ctx: &egui::Context) {
        let (Some(i), Some(s)) = (self.selected, &self.open) else { return };
        let prompt = std::mem::take(&mut self.input);
        if prompt.trim().is_empty() {
            return;
        }
        self.running = Some(runner::send(ctx.clone(), self.sessions[i].id.clone(), s.cwd.clone(), prompt));
    }

    fn poll_runner(&mut self) {
        let Some(rx) = &self.running else { return };
        if let Ok(result) = rx.try_recv() {
            self.running = None;
            self.reload();
            if let Err(e) = result {
                self.error = Some(e);
            }
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Sessions");
            if ui.button("⟳").on_hover_text("Refresh").clicked() {
                self.refresh_list();
            }
        });
        ui.separator();
        if self.sessions.is_empty() {
            ui.label("No sessions found in ~/.claude/projects");
        }
        let mut clicked = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, s) in self.sessions.iter().enumerate() {
                let label = format!("{}\n{}", s.title, short_project(&s.project));
                if ui.selectable_label(self.selected == Some(i), label).clicked() {
                    clicked = Some(i);
                }
            }
        });
        if let Some(i) = clicked {
            self.select(i);
        }
    }

    fn prompt_bar(&mut self, ui: &mut egui::Ui) {
        let id = Id::new("prompt");
        let enabled = self.open.is_some() && self.running.is_none();
        // Consume arrows before the TextEdit sees them, so they drive history instead of the cursor.
        if ui.memory(|m| m.has_focus(id)) {
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                if let Some(p) = self.history.up(&self.input) {
                    self.input = p;
                }
            }
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
                if let Some(p) = self.history.down() {
                    self.input = p;
                }
            }
        }
        ui.horizontal(|ui| {
            let hint = if self.running.is_some() { "Claude is working…" } else { "Prompt (Up/Down for history, Enter to send)" };
            let edit = egui::TextEdit::singleline(&mut self.input)
                .id(id)
                .hint_text(hint)
                .desired_width(ui.available_width() - 60.0);
            let resp = ui.add_enabled(enabled, edit);
            let submitted = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if (ui.add_enabled(enabled, egui::Button::new("Send")).clicked() || submitted) && enabled {
                self.send(ui.ctx());
                resp.request_focus();
            }
            if self.running.is_some() {
                ui.spinner();
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, e);
        }
    }

    fn conversation(&self, ui: &mut egui::Ui) {
        let Some(s) = &self.open else {
            ui.centered_and_justified(|ui| ui.label("Select a session on the left"));
            return;
        };
        egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
            for m in &s.messages {
                let (who, color) = match m.role {
                    Role::User => ("You", Color32::from_rgb(120, 170, 255)),
                    Role::Assistant => ("Claude", Color32::from_rgb(230, 150, 90)),
                };
                ui.label(RichText::new(who).strong().color(color));
                ui.label(&m.text);
                ui.add_space(8.0);
            }
        });
    }

    fn diffs(&self, ui: &mut egui::Ui) {
        ui.heading("Changes");
        ui.separator();
        let Some(s) = &self.open else { return };
        if s.changes.is_empty() {
            ui.label("No code changes in this session");
            return;
        }
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            for c in &s.changes {
                diff_block(ui, c);
                ui.add_space(10.0);
            }
        });
    }
}

fn diff_block(ui: &mut egui::Ui, c: &Change) {
    ui.label(RichText::new(&c.file).strong().monospace());
    let diff = TextDiff::from_lines(&c.old, &c.new);
    let changes: Vec<_> = diff.iter_all_changes().collect();
    for ch in changes.iter().take(MAX_DIFF_LINES) {
        let (sign, color) = match ch.tag() {
            ChangeTag::Delete => ("-", Color32::from_rgb(240, 110, 110)),
            ChangeTag::Insert => ("+", Color32::from_rgb(110, 210, 120)),
            ChangeTag::Equal => (" ", Color32::GRAY),
        };
        let line = format!("{sign} {}", ch.value().trim_end_matches('\n'));
        ui.label(RichText::new(line).monospace().color(color));
    }
    if changes.len() > MAX_DIFF_LINES {
        ui.label(format!("… {} more lines", changes.len() - MAX_DIFF_LINES));
    }
}

/// "-Users-me-Desktop-foo-bar" → "Desktop-foo-bar"
fn short_project(p: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default().replace(['/', '.'], "-");
    p.strip_prefix(&home).map(|s| s.trim_start_matches('-')).unwrap_or(p).to_owned()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_runner();
        egui::Panel::left("sessions").default_size(260.0).resizable(true).show(ui, |ui| self.sidebar(ui));
        egui::Panel::right("diffs").default_size(420.0).resizable(true).show(ui, |ui| self.diffs(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            self.prompt_bar(ui);
            ui.separator();
            self.conversation(ui);
        });
    }
}
