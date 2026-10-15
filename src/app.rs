use crate::history::History;
use crate::runner;
use crate::session::{self, Change, Role, Session, SessionMeta};
use crate::workspaces::Workspaces;
use eframe::egui::{self, Align2, Color32, Id, Key, LayerId, Modifiers, Order, RichText, Sense, Stroke, StrokeKind};
use similar::{ChangeTag, TextDiff};
use std::sync::mpsc::Receiver;

const MAX_DIFF_LINES: usize = 200;
const USER_COLOR: Color32 = Color32::from_rgb(120, 170, 255);

/// Sidebar interactions, collected while drawing and applied afterwards.
enum Action {
    Select(usize),
    Tab(Option<usize>),
    MoveTo(String, usize),
    Remove(String),
    DeleteWorkspace(usize),
}

pub struct App {
    sessions: Vec<SessionMeta>,
    selected: Option<usize>,
    open: Option<Session>,
    error: Option<String>,
    input: String,
    history: History,
    running: Option<Receiver<Result<String, String>>>,
    workspaces: Workspaces,
    /// None = "All sessions", Some(i) = workspace i.
    tab: Option<usize>,
    /// Some while the "+" name field is open.
    new_workspace: Option<String>,
    /// Index into the open session's messages of the "You" prompt pinned above the conversation.
    pinned: Option<usize>,
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
            workspaces: Workspaces::load(&Workspaces::path()),
            tab: None,
            new_workspace: None,
            pinned: None,
        }
    }

    fn save_workspaces(&mut self) {
        if let Err(e) = self.workspaces.save(&Workspaces::path()) {
            self.error = Some(format!("could not save workspaces: {e}"));
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Select(i) => self.select(i),
            Action::Tab(t) => self.tab = t,
            Action::MoveTo(id, w) => {
                self.workspaces.move_to(&id, w);
                self.save_workspaces();
            }
            Action::Remove(id) => {
                self.workspaces.remove(&id);
                self.save_workspaces();
            }
            Action::DeleteWorkspace(w) => {
                self.workspaces.delete(w);
                self.tab = None;
                self.save_workspaces();
            }
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
                self.pinned = None;
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
        let mut actions = Vec::new();
        ui.horizontal(|ui| {
            ui.heading("Sessions");
            if ui.button("⟳").on_hover_text("Refresh").clicked() {
                self.refresh_list();
            }
        });
        self.tabs(ui, &mut actions);
        ui.separator();

        let visible: Vec<usize> = match self.tab.and_then(|w| self.workspaces.list.get(w)) {
            None => (0..self.sessions.len()).collect(),
            Some(w) => (0..self.sessions.len()).filter(|&i| w.sessions.contains(&self.sessions[i].id)).collect(),
        };
        if visible.is_empty() {
            ui.label(if self.tab.is_some() {
                "Empty workspace. Drag sessions onto its tab, or right-click a session → Move to."
            } else {
                "No sessions found in ~/.claude/projects"
            });
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in visible {
                let s = &self.sessions[i];
                let label = format!("{}\n{}", s.title, short_project(&s.project));
                // One widget that senses both: a separate drag-source wrapper would swallow the click.
                let row = ui.selectable_label(self.selected == Some(i), label).interact(Sense::click_and_drag());
                row.dnd_set_drag_payload(s.id.clone());
                if row.dragged() {
                    if let Some(pos) = ui.ctx().pointer_interact_pos() {
                        let painter = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::new("drag-ghost")));
                        let font = egui::FontId::proportional(14.0);
                        painter.text(pos + egui::vec2(14.0, 0.0), Align2::LEFT_CENTER, &s.title, font, USER_COLOR);
                    }
                }
                if row.clicked() {
                    actions.push(Action::Select(i));
                }
                row.context_menu(|ui| {
                    ui.menu_button("Move to", |ui| {
                        if self.workspaces.list.is_empty() {
                            ui.label("No workspaces yet. Create one with +");
                        }
                        for (w, ws) in self.workspaces.list.iter().enumerate() {
                            if ui.button(&ws.name).clicked() {
                                actions.push(Action::MoveTo(s.id.clone(), w));
                            }
                        }
                    });
                    if self.tab.is_some() && ui.button("Remove from workspace").clicked() {
                        actions.push(Action::Remove(s.id.clone()));
                    }
                });
            }
        });
        for a in actions {
            self.apply(a);
        }
    }

    fn tabs(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(self.tab.is_none(), "All sessions").clicked() {
                actions.push(Action::Tab(None));
            }
            for (w, ws) in self.workspaces.list.iter().enumerate() {
                let tab = ui.selectable_label(self.tab == Some(w), &ws.name);
                if tab.clicked() {
                    actions.push(Action::Tab(Some(w)));
                }
                if tab.dnd_hover_payload::<String>().is_some() {
                    ui.painter().rect_stroke(tab.rect, 3.0, Stroke::new(2.0, USER_COLOR), StrokeKind::Outside);
                }
                if let Some(id) = tab.dnd_release_payload::<String>() {
                    actions.push(Action::MoveTo((*id).clone(), w));
                }
                tab.context_menu(|ui| {
                    if ui.button("Delete workspace").clicked() {
                        actions.push(Action::DeleteWorkspace(w));
                    }
                });
            }
            if ui.button("+").on_hover_text("New workspace").clicked() {
                self.new_workspace = Some(String::new());
            }
        });
        let Some(name) = &mut self.new_workspace else { return };
        let id = Id::new("new-workspace");
        let resp = ui.add(egui::TextEdit::singleline(name).id(id).hint_text("Workspace name, Enter to create"));
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        if resp.lost_focus() {
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                if let Some(w) = self.workspaces.add(name) {
                    actions.push(Action::Tab(Some(w)));
                    self.save_workspaces();
                }
            }
            self.new_workspace = None;
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

    fn conversation(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.open else {
            ui.centered_and_justified(|ui| ui.label("Select a session on the left"));
            return;
        };
        // Pinned "You": the prompt of the turn at the top of the viewport (computed last frame).
        let pinned = self.pinned.or_else(|| s.messages.iter().position(|m| m.role == Role::User));
        if let Some(m) = pinned.and_then(|i| s.messages.get(i)) {
            egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(6.0).corner_radius(4.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("You").strong().color(USER_COLOR));
                egui::ScrollArea::vertical().id_salt("pinned").max_height(90.0).show(ui, |ui| ui.label(&m.text));
            });
            ui.add_space(4.0);
        }
        let mut top_prompt = None;
        egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show_viewport(ui, |ui, viewport| {
            let origin = ui.cursor().top();
            for (i, m) in s.messages.iter().enumerate() {
                let (who, color) = match m.role {
                    Role::User => {
                        if ui.cursor().top() - origin <= viewport.min.y + 1.0 {
                            top_prompt = Some(i);
                        }
                        ("You", USER_COLOR)
                    }
                    Role::Assistant => ("Claude", Color32::from_rgb(230, 150, 90)),
                };
                ui.label(RichText::new(who).strong().color(color));
                ui.label(&m.text);
                ui.add_space(8.0);
            }
        });
        if top_prompt != self.pinned {
            self.pinned = top_prompt;
            ui.ctx().request_repaint();
        }
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
