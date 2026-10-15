use crate::diffview::{self, CardEvent};
use crate::goal;
use crate::history::History;
use crate::markdown;
use crate::names::Names;
use crate::runner::{self, RunEvent};
use crate::search::{self, Hit, Source};
use crate::status;
use crate::theme;
use crate::session::{self, Role, Session, SessionMeta};
use crate::workspaces::Workspaces;
use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{self, Align2, Color32, Id, Key, LayerId, Modifiers, Order, RichText, Sense, Stroke, StrokeKind, ThemePreference};
use egui_commonmark::CommonMarkCache;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::mpsc::Receiver;

/// Sidebar interactions, collected while drawing and applied afterwards.
enum Action {
    Select(usize),
    Tab(Option<usize>),
    MoveTo(String, usize),
    Remove(String),
    DeleteWorkspace(usize),
    Rename(String, String),
    CancelRename,
}

#[derive(PartialEq)]
enum RightTab {
    Changes,
    Info,
}

/// Inline rename in progress: which session, the edited text, and whether it was just opened.
struct Renaming {
    id: String,
    text: String,
    fresh: bool,
}

pub struct App {
    sessions: Vec<SessionMeta>,
    selected: Option<usize>,
    open: Option<Session>,
    error: Option<String>,
    input: String,
    history: History,
    running: Option<Receiver<RunEvent>>,
    /// The reply being written by the running claude, and the tool it is starting.
    live: String,
    live_tool: Option<String>,
    workspaces: Workspaces,
    /// None = "All sessions", Some(i) = workspace i.
    tab: Option<usize>,
    /// Some while the "+" name field is open.
    new_workspace: Option<String>,
    /// Index into the open session's messages of the "You" prompt pinned above the conversation.
    pinned: Option<usize>,
    names: Names,
    renaming: Option<Renaming>,
    /// Folder of a new session that has no prompt yet (nothing exists on disk until then).
    new_session: Option<PathBuf>,
    /// Id of a session being created by the running `claude`, to select once it exists.
    creating: Option<String>,
    picker: Option<Receiver<Option<PathBuf>>>,
    /// Set to move keyboard focus to the prompt input on the next frame.
    focus_prompt: bool,
    right_tab: RightTab,
    /// AI-written one-line goals, keyed by session id.
    goals: Names,
    /// Session id whose goal is being summarized, and the pending result.
    summarizing: Option<(String, Receiver<Result<String, String>>)>,
    btw: Option<Btw>,
    /// Text in the Info tab's "ask about this goal" field.
    goal_question: String,
    /// Changes tab shows only this file, when set.
    changes_filter: Option<String>,
    /// Files open read-only in the main area.
    file_tabs: Vec<FileTab>,
    /// Active main tab: None = the conversation ("Main"), Some(i) = file_tabs[i].
    main_tab: Option<usize>,
    md_cache: CommonMarkCache,
    /// Rendered height of each message (0 = not measured yet), so off-screen ones can be skipped.
    heights: Vec<f32>,
    /// Width the heights were measured at.
    heights_width: f32,
    theme: ThemePreference,
    /// Background indexer: fresh session lists + content index, and a channel to poke it.
    snapshots: Receiver<search::Snapshot>,
    poke_index: std::sync::mpsc::Sender<()>,
    index: search::Index,
    /// Bumped on every snapshot, so cached search hits know when they're stale.
    generation: u64,
    search: String,
    /// Live hits for (query, tab, generation).
    hits: Option<(String, Option<usize>, u64, Vec<Hit>)>,
    ai: Option<AiSearch>,
}

/// An Enter-search: Haiku picks sessions from a catalog. Results are kept as session ids, so
/// a list refresh doesn't shift them.
struct AiSearch {
    query: String,
    ids: Vec<String>,
    rx: Option<Receiver<Result<String, String>>>,
    result: Option<Result<Vec<(String, String)>, String>>,
}

/// A file open read-only in the main area. Markdown files can show rendered or as source.
struct FileTab {
    path: String,
    contents: Result<String, String>,
    rendered: bool,
}

/// A side question about the session (like Claude Code's /btw): asked with its context, not added to it.
struct Btw {
    question: String,
    answer: Option<Result<String, String>>,
    rx: Option<Receiver<Result<String, String>>>,
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        let (snapshots, poke_index) = search::spawn_indexer(ctx.clone());
        let mut app = Self {
            sessions: session::list(&session::projects_dir()),
            selected: None,
            open: None,
            error: None,
            input: String::new(),
            history: History::default(),
            running: None,
            live: String::new(),
            live_tool: None,
            workspaces: Workspaces::load(&Workspaces::path()),
            tab: None,
            new_workspace: None,
            pinned: None,
            names: Names::load(&Names::path()),
            renaming: None,
            new_session: None,
            creating: None,
            picker: None,
            focus_prompt: false,
            right_tab: RightTab::Changes,
            goals: Names::load(&Names::goals_path()),
            summarizing: None,
            btw: None,
            goal_question: String::new(),
            changes_filter: None,
            file_tabs: Vec::new(),
            main_tab: None,
            md_cache: CommonMarkCache::default(),
            heights: Vec::new(),
            heights_width: 0.0,
            theme: theme::load(),
            snapshots,
            poke_index,
            index: search::Index::new(),
            generation: 0,
            search: String::new(),
            hits: None,
            ai: None,
        };
        // Persist colors assigned on load to workspaces saved before colors existed.
        if !app.workspaces.list.is_empty() {
            app.save_workspaces();
        }
        app
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
            Action::Rename(id, name) => {
                self.renaming = None;
                self.names.set(&id, &name);
                if let Err(e) = self.names.save(&Names::path()) {
                    self.error = Some(format!("could not save names: {e}"));
                }
            }
            Action::CancelRename => self.renaming = None,
            Action::DeleteWorkspace(w) => {
                self.workspaces.delete(w);
                self.tab = None;
                self.save_workspaces();
            }
        }
    }

    fn select(&mut self, i: usize) {
        self.selected = Some(i);
        self.btw = None;
        self.changes_filter = None;
        self.main_tab = None;
        self.new_session = None;
        self.focus_prompt = true;
        self.reload();
    }

    /// Empty session in `dir`; the first prompt creates it.
    fn start_new_session(&mut self, dir: PathBuf) {
        self.selected = None;
        self.open = Some(Session { cwd: Some(dir.clone()), ..Default::default() });
        self.new_session = Some(dir);
        self.history = History::default();
        self.pinned = None;
        self.error = None;
        self.focus_prompt = true;
    }

    fn poll_btw(&mut self) {
        let Some(b) = &mut self.btw else { return };
        if let Some(Ok(result)) = b.rx.as_ref().map(|rx| rx.try_recv()) {
            b.answer = Some(result);
            b.rx = None;
        }
    }

    fn poll_summary(&mut self) {
        let Some((id, rx)) = &self.summarizing else { return };
        if let Ok(result) = rx.try_recv() {
            let id = id.clone();
            self.summarizing = None;
            match result {
                Ok(goal) => {
                    self.goals.set(&id, &goal);
                    if let Err(e) = self.goals.save(&Names::goals_path()) {
                        self.error = Some(format!("could not save goal: {e}"));
                    }
                }
                Err(e) => self.error = Some(format!("could not summarize: {e}")),
            }
        }
    }

    fn poll_picker(&mut self) {
        let Some(rx) = &self.picker else { return };
        if let Ok(dir) = rx.try_recv() {
            self.picker = None;
            if let Some(dir) = dir {
                self.start_new_session(dir);
            }
        }
    }

    fn reload(&mut self) {
        for tab in &mut self.file_tabs {
            tab.contents = diffview::read(&tab.path);
        }
        let Some(meta) = self.selected.and_then(|i| self.sessions.get(i)) else { return };
        match session::load(&meta.path) {
            Ok(s) => {
                self.history = History::new(s.prompts.clone());
                self.open = Some(s);
                self.pinned = None;
                self.heights.clear();
                self.error = None;
            }
            Err(e) => self.error = Some(format!("could not read {}: {e}", meta.path.display())),
        }
    }

    /// Takes the indexer's latest snapshot: new list (selection kept by id) and content index.
    /// The open session reloads if its file changed elsewhere (e.g. a terminal session).
    fn poll_snapshots(&mut self) {
        let Some(snap) = self.snapshots.try_iter().last() else { return };
        let current = self.selected.map(|i| (self.sessions[i].id.clone(), self.sessions[i].modified));
        self.sessions = snap.sessions;
        self.index = snap.index;
        self.generation += 1;
        self.selected = current.as_ref().and_then(|(id, _)| self.sessions.iter().position(|s| &s.id == id));
        let changed = match (&current, self.selected) {
            (Some((_, before)), Some(i)) => self.sessions[i].modified != *before,
            _ => false,
        };
        if changed && self.running.is_none() {
            self.reload();
        }
    }

    fn send(&mut self, ctx: &egui::Context) {
        let Some(cwd) = self.open.as_ref().map(|s| s.cwd.clone()) else { return };
        let (id, new) = match (&self.new_session, self.selected) {
            (Some(_), _) => (runner::new_session_id(), true),
            (None, Some(i)) => (self.sessions[i].id.clone(), false),
            _ => return,
        };
        let prompt = std::mem::take(&mut self.input);
        if prompt.trim().is_empty() {
            return;
        }
        if new {
            self.creating = Some(id.clone());
            if let Some(w) = self.tab {
                self.workspaces.move_to(&id, w);
                self.save_workspaces();
            }
        }
        if let Some(s) = &mut self.open {
            s.push_prompt(prompt.clone());
        }
        self.history = History::new(self.open.as_ref().map(|s| s.prompts.clone()).unwrap_or_default());
        self.live.clear();
        self.live_tool = None;
        self.running = Some(runner::send(ctx.clone(), id, cwd, prompt, new));
    }

    fn poll_runner(&mut self) {
        let Some(rx) = &self.running else { return };
        let mut done = None;
        for event in rx.try_iter() {
            match event {
                RunEvent::Line(v) => {
                    if let Some(s) = &mut self.open {
                        s.apply(&v);
                    }
                    self.live.clear();
                    self.live_tool = None;
                }
                RunEvent::Delta(t) => self.live.push_str(&t),
                RunEvent::Tool(name) => self.live_tool = Some(name),
                RunEvent::Done(result) => done = Some(result),
            }
        }
        let Some(result) = done else { return };
        self.running = None;
        self.live.clear();
        self.live_tool = None;
        if let Some(id) = self.creating.take() {
            self.sessions = session::list(&session::projects_dir());
            // If claude failed before creating it, stay on the empty session to retry.
            if let Some(i) = self.sessions.iter().position(|s| s.id == id) {
                self.selected = Some(i);
                self.new_session = None;
            }
        }
        // The file now holds what was streamed; reloading makes it canonical.
        if self.new_session.is_none() {
            self.reload();
        }
        let _ = self.poke_index.send(());
        if let Err(e) = result {
            self.error = Some(e);
        }
    }

    fn poll_ai(&mut self) {
        let Some(ai) = &mut self.ai else { return };
        let Some(result) = ai.rx.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
        ai.rx = None;
        ai.result = Some(result.map(|reply| {
            search::parse_ai(&reply, ai.ids.len()).into_iter().map(|(i, why)| (ai.ids[i].clone(), why)).collect()
        }));
    }

    /// Sessions in the active tab (all, or one workspace), newest first.
    fn visible(&self) -> Vec<usize> {
        match self.tab.and_then(|w| self.workspaces.list.get(w)) {
            None => (0..self.sessions.len()).collect(),
            Some(w) => (0..self.sessions.len()).filter(|&i| w.sessions.contains(&self.sessions[i].id)).collect(),
        }
    }

    fn title(&self, i: usize) -> &str {
        let s = &self.sessions[i];
        self.names.get(&s.id).unwrap_or(&s.title)
    }

    /// Rows to list, with a note replacing the folder line: all visible sessions, or search results.
    fn rows(&mut self) -> Vec<(usize, Option<String>)> {
        let visible = self.visible();
        let query = self.search.trim().to_owned();
        if query.is_empty() {
            return visible.into_iter().map(|i| (i, None)).collect();
        }
        if let Some(Ok(picked)) = self.ai.as_ref().filter(|a| a.query == query).and_then(|a| a.result.as_ref()) {
            return picked
                .iter()
                .filter_map(|(id, why)| visible.iter().copied().find(|&i| &self.sessions[i].id == id).map(|i| (i, Some(why.clone()))))
                .collect();
        }
        let fresh = self.hits.as_ref().is_some_and(|(q, t, g, _)| *q == query && *t == self.tab && *g == self.generation);
        if !fresh {
            let candidates: Vec<search::Candidate> = visible
                .iter()
                .map(|&i| search::Candidate {
                    index: i,
                    name: self.title(i),
                    goal: self.goals.get(&self.sessions[i].id),
                    entry: self.index.get(&self.sessions[i].id).map(|e| &**e),
                    modified: self.sessions[i].modified,
                })
                .collect();
            let hits = search::rank(&query, &candidates);
            self.hits = Some((query, self.tab, self.generation, hits));
        }
        let hits = &self.hits.as_ref().unwrap().3;
        hits.iter()
            .map(|h| {
                let note = match h.source {
                    Source::Name => None,
                    Source::Goal => Some("matched in its goal".to_owned()),
                    Source::Content => h.note.clone(),
                };
                (h.index, note)
            })
            .collect()
    }

    /// Enter in the search: ask Haiku which of the 40 most recent visible sessions match.
    fn ask_ai(&mut self, ctx: &egui::Context) {
        let query = self.search.trim().to_owned();
        if query.is_empty() {
            return;
        }
        let recent: Vec<usize> = self.visible().into_iter().take(40).collect();
        let catalog: Vec<_> = recent
            .iter()
            .map(|&i| {
                let id = &self.sessions[i].id;
                let prompts = self.index.get(id).map(|e| e.prompts.clone()).unwrap_or_default();
                (self.title(i).to_owned(), self.goals.get(id).map(str::to_owned), prompts)
            })
            .collect();
        let rx = runner::ask_haiku(ctx.clone(), search::ai_prompt(&query, &catalog));
        let ids = recent.iter().map(|&i| self.sessions[i].id.clone()).collect();
        self.ai = Some(AiSearch { query, ids, rx: Some(rx), result: None });
    }

    /// The search field at the bottom of the sidebar, with the AI search status above it.
    fn search_bar(&mut self, ui: &mut egui::Ui) {
        let query = self.search.trim().to_owned();
        if let Some(ai) = self.ai.as_ref().filter(|a| a.query == query) {
            match &ai.result {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Asking Haiku…").weak());
                    });
                }
                Some(Ok(picked)) if picked.is_empty() => {
                    ui.label(RichText::new("Haiku found no matching session. Edit the query for live results.").weak());
                }
                Some(Ok(_)) => {
                    ui.label(RichText::new("Haiku's picks, best first. Edit the query for live results.").weak().small());
                }
                Some(Err(e)) => {
                    ui.colored_label(theme::palette(ui.visuals()).error, format!("AI search failed: {e}"));
                }
            }
        }
        let edit = egui::TextEdit::singleline(&mut self.search)
            .id(Id::new("search"))
            .hint_text("🔍 Search · Enter asks Haiku")
            .desired_width(f32::INFINITY);
        let resp = ui.add(edit);
        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.search.clear();
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            self.ask_ai(ui.ctx());
            resp.request_focus();
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let mut actions = Vec::new();
        self.tabs(ui, &mut actions);
        ui.add_space(4.0);
        let new_button = egui::Button::new(RichText::new("+  New session").strong()).min_size(egui::vec2(ui.available_width(), 28.0));
        if ui.add_enabled(self.picker.is_none(), new_button).clicked() {
            let start = self.open.as_ref().and_then(|s| s.cwd.clone());
            self.picker = Some(runner::pick_folder(ui.ctx().clone(), start));
        }
        ui.separator();
        egui::Panel::bottom("search")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
            .resizable(false)
            .show(ui, |ui| self.search_bar(ui));

        let rows = self.rows();
        if rows.is_empty() && !self.search.trim().is_empty() {
            ui.label(RichText::new("No matches. Press Enter to ask Haiku.").weak());
        } else if rows.is_empty() {
            ui.label(if self.tab.is_some() {
                "Empty workspace. Drag sessions onto its tab, or right-click a session → Move to."
            } else {
                "No sessions found in ~/.claude/projects"
            });
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, note) in rows {
                let s = &self.sessions[i];
                let title = self.names.get(&s.id).unwrap_or(&s.title);
                if let Some(r) = self.renaming.as_mut().filter(|r| r.id == s.id) {
                    rename_field(ui, r, &s.project, &mut actions);
                    continue;
                }
                let label = format!("{title}\n{}", note.unwrap_or_else(|| short_project(&s.project)));
                // One widget that senses both: a separate drag-source wrapper would swallow the click.
                let bg = ui.painter().add(egui::Shape::Noop);
                let row = ui.selectable_label(self.selected == Some(i), label).interact(Sense::click_and_drag());
                if let Some(c) = self.tab.is_none().then(|| self.workspaces.color_of(&s.id)).flatten() {
                    // Tint behind the row, stripe on its left edge.
                    let full = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), row.rect.y_range());
                    ui.painter().set(bg, egui::Shape::rect_filled(full, 3.0, tint(c, 45)));
                    let stripe = egui::Rect::from_min_size(full.min, egui::vec2(3.0, full.height()));
                    ui.painter().rect_filled(stripe, 1.0, rgb(c));
                }
                row.dnd_set_drag_payload(s.id.clone());
                if row.dragged() {
                    if let Some(pos) = ui.ctx().pointer_interact_pos() {
                        let painter = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::new("drag-ghost")));
                        let font = egui::FontId::proportional(14.0);
                        painter.text(pos + egui::vec2(14.0, 0.0), Align2::LEFT_CENTER, title, font, theme::palette(ui.visuals()).user);
                    }
                }
                if row.clicked() {
                    actions.push(Action::Select(i));
                }
                if row.double_clicked() {
                    self.renaming = Some(Renaming { id: s.id.clone(), text: title.to_owned(), fresh: true });
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
                let bg = ui.painter().add(egui::Shape::Noop);
                let tab = ui.selectable_label(self.tab == Some(w), &ws.name);
                if let Some(c) = ws.color {
                    ui.painter().set(bg, egui::Shape::rect_filled(tab.rect, 3.0, tint(c, 90)));
                    if self.tab == Some(w) {
                        ui.painter().rect_stroke(tab.rect, 3.0, Stroke::new(1.5, rgb(c)), StrokeKind::Inside);
                    }
                }
                if tab.clicked() {
                    actions.push(Action::Tab(Some(w)));
                }
                if tab.dnd_hover_payload::<String>().is_some() {
                    ui.painter().rect_stroke(tab.rect, 3.0, Stroke::new(2.0, theme::palette(ui.visuals()).user), StrokeKind::Outside);
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
        // Consume arrows before the TextEdit sees them, so they drive history instead of the
        // cursor; only for a one-line input, where there are no lines to move between.
        let focused = ui.memory(|m| m.has_focus(id));
        // Plain Enter sends (Shift+Enter is the TextEdit's new line); taken before it sees it.
        let enter = focused
            && ui.input_mut(|i| {
                let hit = !i.modifiers.shift && i.key_pressed(Key::Enter);
                if hit {
                    i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::Enter, .. }));
                }
                hit
            });
        if focused && !self.input.contains('\n') {
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
        let pal = theme::palette(ui.visuals());
        if let Some(e) = &self.error {
            ui.colored_label(pal.error, e);
        }
        egui::Frame::new()
            .fill(pal.prompt_bg)
            .stroke(Stroke::new(1.0, pal.border))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(">").monospace().strong().color(pal.prompt_sign));
                    let hint = match (&self.open, &self.running) {
                        (None, _) => "select a session",
                        (_, Some(_)) => "claude is working…",
                        _ => "type a prompt · enter to send · shift+enter for a new line · up/down for history",
                    };
                    // Grows with the text up to ~10 lines, then scrolls.
                    let row = ui.text_style_height(&egui::TextStyle::Monospace);
                    let width = ui.available_width() - 24.0;
                    let resp = egui::ScrollArea::vertical()
                        .id_salt("prompt-scroll")
                        .max_height(row * 10.0 + 4.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            let edit = egui::TextEdit::multiline(&mut self.input)
                                .id(id)
                                .frame(egui::Frame::NONE)
                                .font(egui::TextStyle::Monospace)
                                .text_color(pal.prompt_text)
                                .hint_text(RichText::new(hint).monospace())
                                .desired_rows(1)
                                .return_key(egui::KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                                .desired_width(width);
                            ui.add_enabled(enabled, edit)
                        })
                        .inner;
                    if self.focus_prompt && enabled {
                        resp.request_focus();
                    }
                    self.focus_prompt = false;
                    if enabled && enter {
                        self.send(ui.ctx());
                    }
                    if self.running.is_some() {
                        ui.spinner();
                    }
                });
            });
    }

    fn conversation(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.open else {
            ui.centered_and_justified(|ui| ui.label("Select a session on the left"));
            return;
        };
        if let Some(dir) = self.new_session.as_ref().filter(|_| s.messages.is_empty()) {
            ui.centered_and_justified(|ui| {
                let status = if self.running.is_some() { "Starting…" } else { "Type your first prompt below." };
                ui.label(format!("New session in {}\n\n{status}", dir.display()));
            });
            return;
        }
        let pal = theme::palette(ui.visuals());
        // Pinned "You": the prompt of the turn at the top of the viewport (computed last frame).
        let pinned = self.pinned.or_else(|| s.messages.iter().position(|m| m.role == Role::User));
        if let Some(m) = pinned.and_then(|i| s.messages.get(i)) {
            egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(6.0).corner_radius(4.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("You").strong().color(pal.user));
                egui::ScrollArea::vertical()
                    .id_salt("pinned")
                    .max_height(90.0)
                    .show(ui, |ui| ui.push_id("pinned", |ui| markdown::show(ui, &mut self.md_cache, &m.text)));
            });
            ui.add_space(4.0);
        }
        // Heights are measured at one width; a resize measures again.
        let width = ui.available_width();
        if (width - self.heights_width).abs() > 0.5 {
            self.heights.clear();
            self.heights_width = width;
        }
        self.heights.resize(s.messages.len(), 0.0);
        let mut top_prompt = None;
        egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show_viewport(ui, |ui, viewport| {
            let origin = ui.cursor().top();
            // Render only messages near the viewport; the rest just take their remembered height.
            let near = viewport.expand2(egui::vec2(0.0, 400.0));
            for (i, m) in s.messages.iter().enumerate() {
                let y = ui.cursor().top() - origin;
                if m.role == Role::User && y <= viewport.min.y + 1.0 {
                    top_prompt = Some(i);
                }
                let h = self.heights[i];
                if h > 0.0 && (y + h < near.min.y || y > near.max.y) {
                    ui.allocate_space(egui::vec2(ui.available_width(), h));
                } else {
                    let rect = ui.push_id(i, |ui| message(ui, &mut self.md_cache, m, &pal)).response.rect;
                    self.heights[i] = rect.height().max(1.0);
                }
            }
            if self.running.is_some() {
                // The reply as it is being written, then what Claude is doing right now.
                if !self.live.is_empty() {
                    let m = session::Message { role: Role::Assistant, text: self.live.clone() };
                    ui.push_id("live", |ui| message(ui, &mut self.md_cache, &m, &pal));
                }
                ui.horizontal(|ui| {
                    ui.spinner();
                    let doing = match &self.live_tool {
                        Some(tool) => format!("⚙ {tool}…"),
                        None if self.live.is_empty() => "Claude is working…".to_owned(),
                        None => "Writing…".to_owned(),
                    };
                    ui.label(RichText::new(doing).weak().monospace());
                });
            }
        });
        if top_prompt != self.pinned {
            self.pinned = top_prompt;
            ui.ctx().request_repaint();
        }
    }

    /// A file tab: Markdown files render, with a toggle to their source; others show as source.
    fn file_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let tab = &mut self.file_tabs[i];
        let text = match (&tab.contents, markdown::is_markdown(&tab.path)) {
            (Ok(text), true) => text,
            _ => return diffview::file_view(ui, &tab.path, &tab.contents),
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab.rendered, true, "Rendered");
            ui.selectable_value(&mut tab.rendered, false, "Source");
        });
        if !tab.rendered {
            return diffview::file_view(ui, &tab.path, &tab.contents);
        }
        ui.label(RichText::new(&tab.path).weak().monospace());
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt(("md-file", i)).auto_shrink(false).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| markdown::show(ui, &mut self.md_cache, text));
        });
    }

    /// Dark / Light / System, saved to sessman's settings.
    fn theme_switch(&mut self, ui: &mut egui::Ui) {
        let before = self.theme;
        // Right-to-left layout: listed in reverse.
        ui.selectable_value(&mut self.theme, ThemePreference::System, "💻").on_hover_text("Follow the system appearance");
        ui.selectable_value(&mut self.theme, ThemePreference::Light, "☀").on_hover_text("Light");
        ui.selectable_value(&mut self.theme, ThemePreference::Dark, "🌙").on_hover_text("Dark");
        if self.theme != before {
            ui.ctx().set_theme(self.theme);
            if let Err(e) = theme::save(self.theme) {
                self.error = Some(format!("could not save the theme: {e}"));
            }
        }
    }

    fn right_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.right_tab, RightTab::Changes, RichText::new("Changes").heading());
            ui.selectable_value(&mut self.right_tab, RightTab::Info, RichText::new("Info").heading());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.theme_switch(ui));
        });
        ui.separator();
        match self.right_tab {
            RightTab::Changes => self.diffs(ui),
            RightTab::Info => self.info(ui),
        }
    }

    fn info(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.open else {
            ui.label("Select a session to see its info");
            return;
        };
        let id = self.selected.map(|i| self.sessions[i].id.clone());
        let mut start_summary = false;
        let mut toggled = None;
        let (mut ask, mut dismiss) = (false, false);
        // Questions about the goal sit at the very bottom, answered with the session's context
        // but not added to it; the answer shows just above the field.
        if id.is_some() && !s.messages.is_empty() {
            egui::Panel::bottom("goal-question")
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
                .show_separator_line(false)
                .resizable(false)
                .show(ui, |ui| {
                    if let Some(b) = &self.btw {
                        egui::Frame::new()
                            .fill(ui.visuals().faint_bg_color)
                            .stroke(Stroke::new(1.0, theme::palette(ui.visuals()).note))
                            .inner_margin(8.0)
                            .corner_radius(4.0)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&b.question).italics());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                        dismiss = ui.small_button("Dismiss").clicked();
                                    });
                                });
                                egui::ScrollArea::vertical().id_salt("goal-answer").max_height(260.0).show(ui, |ui| match &b.answer {
                                    None => {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label("Thinking (not added to the session)…");
                                        });
                                    }
                                    Some(Ok(a)) => markdown::show(ui, &mut self.md_cache, a),
                                    Some(Err(e)) => {
                                        ui.colored_label(theme::palette(ui.visuals()).error, e);
                                    }
                                });
                            });
                        ui.add_space(4.0);
                    }
                    let asking = self.btw.as_ref().is_some_and(|b| b.answer.is_none());
                    let field = egui::TextEdit::singleline(&mut self.goal_question)
                        .hint_text("Ask about this goal, Enter to send")
                        .desired_width(f32::INFINITY);
                    let resp = ui.add_enabled(!asking, field);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.goal_question.trim().is_empty() {
                        ask = true;
                    }
                });
        }
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            section(ui, "Goal");
            let goal = id.as_deref().and_then(|id| self.goals.get(id));
            match (goal, &id) {
                (Some(raw), _) => {
                    let g = goal::parse(raw);
                    ui.label(&g.text);
                    if !g.items.is_empty() {
                        ui.add_space(4.0);
                        let done = g.items.iter().filter(|(d, _)| *d).count();
                        ui.label(RichText::new(format!("{done} of {} done", g.items.len())).weak().small());
                    }
                    for (i, (done, text)) in g.items.into_iter().enumerate() {
                        let mut checked = done;
                        if ui.checkbox(&mut checked, text).changed() {
                            toggled = Some(i);
                        }
                    }
                }
                (None, _) if s.prompts.is_empty() => {
                    ui.label("No prompts yet");
                }
                (None, _) => {
                    let first: String = s.prompts[0].chars().take(280).collect();
                    ui.label(RichText::new(format!("First prompt: {first}…")).weak());
                }
            }
            if let Some(id) = id.as_ref().filter(|_| !s.prompts.is_empty()) {
                if self.summarizing.as_ref().is_some_and(|(sid, _)| sid == id) {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Summarizing…");
                    });
                } else {
                    let label = if goal.is_some() { "Summarize again" } else { "Summarize goal" };
                    let btn = ui.button(label).on_hover_text("Asks Claude Haiku for the goal and a checklist of subgoals (uses a little of your usage)");
                    start_summary = btn.clicked();
                }
            }

            section(ui, "Model");
            ui.label(RichText::new(s.model.as_deref().unwrap_or("unknown yet")).monospace());

            section(ui, "Context");
            let exact = id.as_ref().and_then(|id| status::load(&status::dir().join(format!("{id}.json"))));
            let window_size = exact.as_ref().and_then(|(v, _)| v["context_window"]["context_window_size"].as_u64());
            if s.context_tokens == 0 {
                ui.label("No replies yet");
            } else {
                let window = window_size.unwrap_or_else(|| context_window(s.model.as_deref()));
                let used = s.context_tokens as f32 / window as f32;
                ui.add(egui::ProgressBar::new(used.min(1.0)).text(format!(
                    "{} / {} tokens ({:.0}%)",
                    short_tokens(s.context_tokens),
                    short_tokens(window),
                    used * 100.0
                )));
                ui.label(RichText::new("As of the latest reply").weak().small());
            }

            section(ui, "Usage limits");
            if limits_ui(ui) {
                match status::enable() {
                    Ok(()) => self.error = None,
                    Err(e) => self.error = Some(format!("could not enable status line: {e}")),
                }
            }
        });
        if let (Some(i), Some(id)) = (toggled, &id) {
            let raw = goal::toggle(self.goals.get(id).unwrap_or_default(), i);
            self.goals.set(id, &raw);
            if let Err(e) = self.goals.save(&Names::goals_path()) {
                self.error = Some(format!("could not save goal: {e}"));
            }
        }
        if dismiss {
            self.btw = None;
        }
        if let (true, Some(id)) = (ask, &id) {
            let question = std::mem::take(&mut self.goal_question).trim().to_owned();
            let rx = runner::btw(ui.ctx().clone(), id.clone(), s.cwd.clone(), goal_question_prompt(self.goals.get(id), &question));
            self.btw = Some(Btw { question, answer: None, rx: Some(rx) });
        }
        if let (true, Some(id)) = (start_summary, id) {
            let rx = runner::summarize_goal(ui.ctx().clone(), session::digest(&s.messages, 400, 12_000));
            self.summarizing = Some((id, rx));
        }
    }

    fn open_file(&mut self, path: &str) {
        let i = match self.file_tabs.iter().position(|t| t.path == path) {
            Some(i) => i,
            None => {
                let tab = FileTab { path: path.to_owned(), contents: diffview::read(path), rendered: markdown::is_markdown(path) };
                self.file_tabs.push(tab);
                self.file_tabs.len() - 1
            }
        };
        self.main_tab = Some(i);
    }

    fn close_file(&mut self, i: usize) {
        drop(self.file_tabs.remove(i));
        self.main_tab = match self.main_tab {
            Some(t) if t == i => None,
            Some(t) if t > i => Some(t - 1),
            t => t,
        };
    }

    fn diffs(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.open else { return };
        if s.changes.is_empty() {
            ui.label("No code changes in this session");
            return;
        }
        let files = diffview::group(&s.changes);
        let (mut toggle, mut open) = (None, None);
        // File list at the bottom: click filters the cards above, double-click opens the file.
        egui::Panel::bottom("changed-files").resizable(true).default_size(170.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Files ({})", files.len())).strong());
                if self.changes_filter.is_some() && ui.small_button("Show all").clicked() {
                    toggle = self.changes_filter.clone();
                }
            });
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                for f in &files {
                    let (name, dir) = diffview::split_path(&f.file);
                    let selected = self.changes_filter.as_deref() == Some(f.file.as_str());
                    let row = ui
                        .horizontal(|ui| {
                            let r = ui.selectable_label(selected, RichText::new(name).monospace());
                            ui.label(RichText::new(format!("+{} -{}", f.added, f.removed)).weak().small());
                            r
                        })
                        .inner
                        .on_hover_text(format!("{dir}\nClick: show its changes · Double-click: open it"));
                    if row.double_clicked() {
                        open = Some(f.file.clone());
                    } else if row.clicked() {
                        toggle = Some(f.file.clone());
                    }
                }
            });
        });
        // Both directions: long unwrapped diff lines scroll instead of widening the panel.
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            for f in files.iter().filter(|f| self.changes_filter.as_ref().is_none_or(|x| *x == f.file)) {
                match diffview::file_card(ui, f, &s.changes) {
                    CardEvent::DoubleClicked => open = Some(f.file.clone()),
                    CardEvent::Clicked => toggle = Some(f.file.clone()),
                    CardEvent::None => {}
                }
                ui.add_space(8.0);
            }
        });
        if let Some(file) = toggle {
            self.changes_filter = if self.changes_filter.as_ref() == Some(&file) { None } else { Some(file) };
        }
        if let Some(file) = open {
            self.changes_filter = Some(file.clone());
            self.open_file(&file);
        }
    }

    /// "Main" plus one tab per open file, above the main area.
    fn main_tabs(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(self.main_tab.is_none(), RichText::new("Main").strong()).clicked() {
                self.main_tab = None;
            }
            for (i, t) in self.file_tabs.iter().enumerate() {
                let (name, _) = diffview::split_path(&t.path);
                let tab = ui.selectable_label(self.main_tab == Some(i), RichText::new(name).monospace()).on_hover_text(&t.path);
                if tab.clicked() {
                    self.main_tab = Some(i);
                }
                if ui.small_button("x").on_hover_text("Close").clicked() {
                    close = Some(i);
                }
                ui.add_space(6.0);
            }
        });
        if let Some(i) = close {
            self.close_file(i);
        }
    }
}

/// One message as a card: role label and accent stripe, then its rendered Markdown.
/// Messages that are only tool calls show as a single dimmed line.
fn message(ui: &mut egui::Ui, cache: &mut CommonMarkCache, m: &session::Message, pal: &theme::Palette) {
    if m.role == Role::Assistant && m.text.lines().all(|l| l.starts_with("[tool:")) {
        egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 0, top: 0, bottom: 6 }).show(ui, |ui| markdown::show(ui, cache, &m.text));
        return;
    }
    let (who, accent) = match m.role {
        Role::User => ("You", pal.user),
        Role::Assistant => ("Claude", pal.assistant),
    };
    let fill = match m.role {
        Role::User => accent.gamma_multiply(0.08),
        Role::Assistant => ui.visuals().faint_bg_color,
    };
    let card = egui::Frame::new()
        .fill(fill)
        .corner_radius(6.0)
        .inner_margin(egui::Margin { left: 14, right: 10, top: 6, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(who).strong().color(accent));
            markdown::show(ui, cache, &m.text);
        });
    let r = card.response.rect;
    let stripe = egui::Rect::from_min_max(r.min, egui::pos2(r.left() + 3.0, r.bottom()));
    ui.painter().rect_filled(stripe, egui::CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, accent);
    ui.add_space(6.0);
}

/// Editable name in place of a session row. Enter or clicking away saves; Escape cancels.
fn rename_field(ui: &mut egui::Ui, r: &mut Renaming, project: &str, actions: &mut Vec<Action>) {
    let id = Id::new("rename");
    let mut out = egui::TextEdit::singleline(&mut r.text).id(id).desired_width(f32::INFINITY).show(ui);
    if r.fresh {
        // Like renaming a folder: focused, with the whole name selected.
        r.fresh = false;
        out.response.request_focus();
        let all = CCursorRange::two(CCursor::new(0), CCursor::new(r.text.chars().count()));
        out.state.cursor.set_char_range(Some(all));
        out.state.store(ui.ctx(), id);
    }
    ui.label(RichText::new(short_project(project)).weak());
    if out.response.lost_focus() {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            actions.push(Action::CancelRename);
        } else {
            actions.push(Action::Rename(r.id.clone(), r.text.clone()));
        }
    }
}


/// Account usage limits from the latest status-line payload. Returns true if "Enable" was clicked.
fn limits_ui(ui: &mut egui::Ui) -> bool {
    // The file changes whenever Claude Code refreshes its status line; re-read every few seconds.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
    match status::setup() {
        status::Setup::Other(cmd) => {
            ui.label(RichText::new(format!("Claude Code limits are only shared with the status line, and yours is already set to `{cmd}`. sessman won't replace it.")).weak());
            return false;
        }
        status::Setup::NotSet => {
            ui.label(RichText::new("Claude Code shares usage limits only with its status line. Enable sessman's status line to see them here (sets statusLine in ~/.claude/settings.json).").weak());
            return ui.button("Enable").clicked();
        }
        status::Setup::Enabled => {}
    }
    let Some((v, at)) = status::load(&status::dir().join("latest.json")) else {
        ui.label(RichText::new("Enabled. Limits appear after your next reply in an interactive Claude Code session (Pro/Max plans only).").weak());
        return false;
    };
    let now = SystemTime::now();
    for w in status::limits(&v) {
        let reset = w.resets_at.map(|t| UNIX_EPOCH + std::time::Duration::from_secs(t));
        if reset.is_some_and(|r| r <= now) {
            continue; // window has reset since the payload was written
        }
        ui.label(w.label);
        let text = match reset.and_then(|r| r.duration_since(now).ok()) {
            Some(d) => format!("{:.0}% · resets in {}", w.used, short_duration(d)),
            None => format!("{:.0}%", w.used),
        };
        ui.add(egui::ProgressBar::new((w.used / 100.0) as f32).text(text));
    }
    let age = now.duration_since(at).unwrap_or_default();
    ui.label(RichText::new(format!("Updated {} ago", short_duration(age))).weak().small());
    false
}

/// 8000s → "2h 13m"
fn short_duration(d: std::time::Duration) -> String {
    let m = d.as_secs() / 60;
    match (m / 1440, m / 60 % 24, m % 60) {
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (days, h, _) => format!("{days}d {h}h"),
    }
}

/// The question sent to the forked session, with the current goal and checklist for focus.
fn goal_question_prompt(goal: Option<&str>, question: &str) -> String {
    match goal {
        Some(raw) => format!(
            "Side question about this session (answer briefly; do not change any files).\n\
             The session's goal and subgoal checklist:\n{raw}\n\nQuestion: {question}"
        ),
        None => format!("Side question about this session (answer briefly; do not change any files).\n\nQuestion: {question}"),
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).strong());
    ui.add_space(2.0);
}

/// Fallback when there is no status-line payload for the session. Per the model docs, 1M is the
/// default for current models; Claude 3 and Opus/Sonnet up to 4.6 have 200k unless `[1m]`.
fn context_window(model: Option<&str>) -> u64 {
    let m = model.unwrap_or_default();
    let old = m.starts_with("claude-3") || ["-4-0", "-4-1", "-4-5", "-4-6", "-4-2025"].iter().any(|v| m.contains(v));
    if old && !m.contains("[1m]") { 200_000 } else { 1_000_000 }
}

/// 180234 → "180k"
fn short_tokens(n: u64) -> String {
    if n >= 1_000_000 { format!("{:.1}M", n as f64 / 1e6) } else { format!("{}k", n / 1000) }
}

fn rgb([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn tint([r, g, b]: [u8; 3], alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// "-Users-me-Desktop-foo-bar" → "Desktop-foo-bar"
fn short_project(p: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default().replace(['/', '.'], "-");
    p.strip_prefix(&home).map(|s| s.trim_start_matches('-')).unwrap_or(p).to_owned()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_runner();
        self.poll_picker();
        self.poll_summary();
        self.poll_btw();
        self.poll_snapshots();
        self.poll_ai();
        egui::Panel::left("sessions").default_size(260.0).resizable(true).show(ui, |ui| self.sidebar(ui));
        egui::Panel::right("diffs").default_size(420.0).resizable(true).show(ui, |ui| self.right_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            if !self.file_tabs.is_empty() {
                egui::Panel::top("main-tabs").show_separator_line(true).resizable(false).show(ui, |ui| self.main_tabs(ui));
            }
            egui::Panel::bottom("prompt")
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
                .show_separator_line(false)
                .resizable(false)
                .show(ui, |ui| self.prompt_bar(ui));
            match self.main_tab.filter(|&i| i < self.file_tabs.len()) {
                Some(i) => self.file_tab(ui, i),
                None => self.conversation(ui),
            }
            // A plain click anywhere in the main area goes to the input, like a terminal.
            let area = ui.max_rect();
            if ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|p| area.contains(p))) {
                self.focus_prompt = true;
                ui.ctx().request_repaint();
            }
        });
    }
}
