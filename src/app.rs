use crate::diffview::{self, CardEvent};
use crate::dictation;
use crate::find::{self, Find};
use crate::goal;
use crate::history::History;
use crate::markdown;
use crate::names::Names;
use crate::queue::Queue;
use crate::refine;
use crate::runner::{self, RunEvent};
use crate::search::{self, Hit, Source};
use crate::session::{self, Role, Session, SessionMeta};
use crate::settings::{self, Settings};
use crate::state::{self, OpenFile, State, View, Views};
use crate::status;
use crate::theme;
use crate::workspaces::{self, Workspaces};
use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{self, Align2, Color32, Id, Key, LayerId, Modifiers, Order, RichText, Sense, Stroke, StrokeKind};
use egui_commonmark::CommonMarkCache;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Height of the bar at the top of the window, where the macOS window buttons sit.
pub const TITLE_BAR_HEIGHT: f32 = 28.0;

/// Sidebar interactions, collected while drawing and applied afterwards.
enum Action {
    Select(usize),
    Tab(Option<usize>),
    MoveTo(String, usize),
    Remove(String),
    DeleteWorkspace(usize),
    Rename(String, String),
    CancelRename,
    /// Back to a new session whose first prompt is running (it isn't on disk yet).
    OpenPending(String),
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
    ctx: egui::Context,
    sessions: Vec<SessionMeta>,
    selected: Option<usize>,
    open: Option<Session>,
    error: Option<String>,
    input: String,
    history: History,
    /// Prompts running right now, by session id; each session can run on its own.
    runs: HashMap<String, Run>,
    /// Prompts waiting for a session's run to end, by session id.
    queues: HashMap<String, Queue>,
    /// Sessions whose last run was stopped, with what Claude had written of its reply.
    interrupted: HashMap<String, String>,
    workspaces: Workspaces,
    /// None = "All sessions", Some(i) = workspace i.
    tab: Option<usize>,
    /// Some while the "+" name field is open.
    new_workspace: Option<String>,
    /// Workspace waiting for "Delete?" to be confirmed.
    confirm_delete: Option<usize>,
    /// Index into the open session's messages of the "You" prompt pinned above the conversation.
    pinned: Option<usize>,
    /// Whether a "You" prompt was on screen in the conversation last frame; the pinned one hides then.
    inline_you: bool,
    names: Names,
    renaming: Option<Renaming>,
    /// Folder of a new session that has no prompt yet (nothing exists on disk until then).
    new_session: Option<PathBuf>,
    /// Id given to the new session's first prompt, to select it once it exists.
    new_id: Option<String>,
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
    /// Files open read-only in the main area, for the open session.
    file_tabs: Vec<FileTab>,
    /// Active main tab: None = the conversation ("Main"), Some(i) = file_tabs[i].
    main_tab: Option<usize>,
    /// File tabs, draft and filter of the sessions not on screen.
    views: Views,
    md_cache: CommonMarkCache,
    /// Rendered height of each message (0 = not measured yet), so off-screen ones can be skipped.
    heights: Vec<f32>,
    /// Width the heights were measured at.
    heights_width: f32,
    settings: Settings,
    /// The Settings tab is open in the main area, and whether it is the active tab.
    settings_tab: bool,
    settings_active: bool,
    settings_filter: String,
    /// Prompt input height set by dragging its top edge (None = grows with the text), and
    /// whether it is collapsed to one line.
    prompt_height: Option<f32>,
    prompt_collapsed: bool,
    /// Height the input had last frame, where a drag starts from.
    prompt_measured: f32,
    /// Height of all the text in the input, measured last frame.
    prompt_text_height: f32,
    refine: Option<Refining>,
    /// Text before the last refine, and the refined text (undo applies while it is unchanged).
    refine_undo: Option<(String, String)>,
    /// Microphone dictation into the input (the mic button).
    dictation: Dictation,
    /// The "download the Whisper model?" modal, shown when the mic is used without the model.
    model_prompt: Option<ModelPrompt>,
    /// Recent microphone levels (0..1, oldest first) for the meter shown while recording.
    mic_levels: std::collections::VecDeque<f32>,
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
    /// Last state written, to skip rewriting an unchanged file.
    saved_state: String,
    /// Find in the main area (Ctrl+F): the conversation or the open file tab.
    find: Find,
}

/// A running `claude -p` for one session: its events, the reply being written, the tool it
/// is starting, and what is needed to stop it or send the next queued prompt.
struct Run {
    rx: Receiver<RunEvent>,
    live: String,
    tool: Option<String>,
    new: bool,
    cwd: Option<PathBuf>,
    prompt: String,
    stop: runner::Stop,
    /// The indexer was poked once the new session's file appeared.
    poked: bool,
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

/// Refine prompt in progress: waiting for the helper model, or showing its questions.
struct Refining {
    original: String,
    rx: Option<Receiver<Result<String, String>>>,
    /// Questions with the chosen answer (None = skipped) and the "Other" text.
    questions: Vec<(refine::Question, Option<String>, String)>,
    error: Option<String>,
}

/// Dictation state: recording the microphone, or waiting for the transcript.
enum Dictation {
    Idle,
    Recording(dictation::Recorder),
    Transcribing(Receiver<Result<String, String>>),
}

/// Bars in the microphone level meter, and the width of the meter with its dot and timer.
const MIC_BARS: usize = 14;
const MIC_METER_WIDTH: f32 = 120.0;

/// What shows while recording: a pulsing red dot, the time so far, and a level meter that
/// scrolls right to left, so it is clear the microphone is on and hearing something.
fn recording_meter(ui: &mut egui::Ui, recorder: &dictation::Recorder, levels: &mut std::collections::VecDeque<f32>, red: Color32) {
    // A new bar every 60 ms, holding the loudest sample since the last one.
    let now = ui.input(|i| i.time);
    let last = ui.data(|d| d.get_temp::<f64>(Id::new("mic-sampled"))).unwrap_or(0.0);
    if now - last >= 0.06 {
        levels.push_back(recorder.take_level());
        while levels.len() > MIC_BARS {
            levels.pop_front();
        }
        ui.data_mut(|d| d.insert_temp(Id::new("mic-sampled"), now));
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));

    let row = ui.text_style_height(&egui::TextStyle::Body);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(MIC_METER_WIDTH, row), Sense::hover());
    let painter = ui.painter_at(rect);
    let mid = rect.center().y;
    // Dot: breathes in size and brightness.
    let pulse = 0.5 + 0.5 * (now * std::f64::consts::TAU / 1.2).sin() as f32;
    let dot = egui::pos2(rect.left() + 6.0, mid);
    painter.circle_filled(dot, 5.0 + 2.5 * pulse, red.gamma_multiply(0.25 * pulse));
    painter.circle_filled(dot, 4.0, red.gamma_multiply(0.7 + 0.3 * pulse));
    // Timer.
    let secs = recorder.elapsed().as_secs();
    let timer = painter.text(
        egui::pos2(dot.x + 10.0, mid),
        Align2::LEFT_CENTER,
        format!("{}:{:02}", secs / 60, secs % 60),
        egui::FontId::monospace(row * 0.8),
        red,
    );
    // Meter: thin rounded bars, centered vertically, the newest on the right.
    let (left, right) = (timer.right() + 8.0, rect.right() - 4.0);
    let step = (right - left) / MIC_BARS as f32;
    let width = (step * 0.55).max(1.5);
    for (i, &level) in levels.iter().enumerate() {
        let height = 2.0 + (row - 4.0) * level;
        let x = left + step * (i as f32 + 0.5);
        let bar = egui::Rect::from_center_size(egui::pos2(x, mid), egui::vec2(width, height));
        painter.rect_filled(bar, width / 2.0, red.gamma_multiply(0.45 + 0.55 * level));
    }
}

/// The Whisper model modal: asking first, downloading, then done or failed.
enum ModelPrompt {
    Ask,
    Downloading(std::process::Child),
    Done,
    Failed(String),
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        let (snapshots, poke_index) = search::spawn_indexer(ctx.clone());
        let settings = Settings::load(&settings::path());
        let mut app = Self {
            ctx: ctx.clone(),
            sessions: session::list(&session::projects_dir()),
            selected: None,
            open: None,
            error: None,
            input: String::new(),
            history: History::default(),
            runs: HashMap::new(),
            queues: HashMap::new(),
            interrupted: HashMap::new(),
            workspaces: Workspaces::load(&Workspaces::path()),
            tab: None,
            new_workspace: None,
            confirm_delete: None,
            pinned: None,
            inline_you: false,
            names: Names::load(&Names::path()),
            renaming: None,
            new_session: None,
            new_id: None,
            focus_prompt: false,
            right_tab: RightTab::Changes,
            goals: Names::load(&Names::goals_path()),
            summarizing: None,
            btw: None,
            goal_question: String::new(),
            changes_filter: None,
            file_tabs: Vec::new(),
            main_tab: None,
            views: Views::default(),
            md_cache: CommonMarkCache::default(),
            heights: Vec::new(),
            heights_width: 0.0,
            settings,
            settings_tab: false,
            settings_active: false,
            settings_filter: String::new(),
            prompt_height: None,
            prompt_collapsed: false,
            prompt_measured: 0.0,
            prompt_text_height: 0.0,
            refine: None,
            refine_undo: None,
            dictation: Dictation::Idle,
            model_prompt: None,
            mic_levels: std::collections::VecDeque::new(),
            snapshots,
            poke_index,
            index: search::Index::new(),
            generation: 0,
            search: String::new(),
            hits: None,
            ai: None,
            saved_state: String::new(),
            find: Find::default(),
        };
        app.apply_settings();
        // Persist colors assigned on load to workspaces saved before colors existed.
        if !app.workspaces.list.is_empty() {
            app.save_workspaces();
        }
        if app.settings.restore_state {
            app.restore(State::load(&state::path()));
        }
        app
    }

    /// Puts back what was open when sessman last closed.
    fn restore(&mut self, mut st: State) {
        st.prune(|id| self.sessions.iter().any(|s| s.id == id));
        self.tab = st.tab.as_ref().and_then(|name| self.workspaces.list.iter().position(|w| &w.name == name));
        self.views = Views(st.views);
        self.queues = st.queues;
        self.right_tab = if st.info_tab { RightTab::Info } else { RightTab::Changes };
        self.search = st.search;
        self.prompt_height = st.prompt_height;
        self.prompt_collapsed = st.prompt_collapsed;
        if let Some(i) = st.open.and_then(|id| self.sessions.iter().position(|s| s.id == id)) {
            self.select(i);
        }
    }

    /// Everything `restore` needs, for the state file.
    fn state(&self) -> State {
        let mut views = self.views.0.clone();
        if let Some(key) = self.view_key() {
            let mut v = self.current_view();
            v.draft = self.input.clone();
            if !v.is_empty() {
                views.insert(key, v);
            }
        }
        State {
            tab: self.tab.and_then(|w| self.workspaces.list.get(w)).map(|w| w.name.clone()),
            open: self.new_session.is_none().then(|| self.open_id()).flatten(),
            views,
            queues: self.queues.iter().filter(|(_, q)| !q.is_empty()).map(|(k, q)| (k.clone(), q.clone())).collect(),
            info_tab: self.right_tab == RightTab::Info,
            search: self.search.clone(),
            prompt_height: self.prompt_height,
            prompt_collapsed: self.prompt_collapsed,
        }
    }

    fn save_state(&mut self) {
        let st = self.state();
        let Ok(text) = serde_json::to_string(&st) else { return };
        if text == self.saved_state {
            return;
        }
        match st.save(&state::path()) {
            Ok(()) => self.saved_state = text,
            Err(e) => self.error = Some(format!("could not save the app state: {e}")),
        }
    }

    fn apply_settings(&mut self) {
        self.settings.apply(&self.ctx);
        runner::configure(runner::Claude::from_settings(&self.settings));
        markdown::SHOW_TOOLS.store(self.settings.show_tool_lines, std::sync::atomic::Ordering::Relaxed);
    }

    fn save_workspaces(&mut self) {
        if let Err(e) = self.workspaces.save(&Workspaces::path()) {
            self.error = Some(format!("could not save workspaces: {e}"));
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Select(i) => self.select(i),
            Action::Tab(t) => self.switch_tab(t),
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
            Action::DeleteWorkspace(w) if self.settings.confirm_delete_workspace => self.confirm_delete = Some(w),
            Action::DeleteWorkspace(w) => self.delete_workspace(w),
            Action::OpenPending(id) => self.open_pending(&id),
        }
    }

    fn delete_workspace(&mut self, w: usize) {
        let showing = self.tab == Some(w);
        self.workspaces.delete(w);
        self.save_workspaces();
        if showing {
            self.tab = None;
            self.open_tab();
        } else if let Some(t) = self.tab.filter(|&t| t > w) {
            self.tab = Some(t - 1);
        }
    }

    /// Key of the session in the main area, for its view: its id, or the folder of a new
    /// session that has no id yet.
    fn view_key(&self) -> Option<String> {
        match &self.new_session {
            Some(dir) => Some(self.new_id.clone().unwrap_or_else(|| format!("new:{}", dir.display()))),
            None => self.selected.map(|i| self.sessions[i].id.clone()),
        }
    }

    fn current_view(&self) -> View {
        View {
            files: self.file_tabs.iter().map(|t| OpenFile { path: t.path.clone(), rendered: t.rendered }).collect(),
            active: self.main_tab,
            draft: if self.settings.drafts_per_session { self.input.clone() } else { String::new() },
            changes_filter: self.changes_filter.clone(),
        }
    }

    /// Before the main area shows another session: keeps this one's file tabs, draft and filter,
    /// and brings back the other's.
    fn switch_view(&mut self, to: Option<String>) {
        let from = self.view_key();
        if from == to {
            return;
        }
        let current = self.current_view();
        let v = self.views.switch(from.as_deref(), to.as_deref(), current);
        self.file_tabs = v
            .files
            .into_iter()
            .map(|f| FileTab { contents: diffview::read(&f.path), path: f.path, rendered: f.rendered })
            .collect();
        self.main_tab = v.active.filter(|&i| i < self.file_tabs.len());
        self.changes_filter = v.changes_filter;
        if self.settings.drafts_per_session {
            self.input = v.draft;
        }
        self.refine = None;
        self.refine_undo = None;
        self.settings_active = false;
    }

    fn select(&mut self, i: usize) {
        let id = self.sessions[i].id.clone();
        self.switch_view(Some(id.clone()));
        self.selected = Some(i);
        self.btw = None;
        self.new_session = None;
        self.focus_prompt = true;
        if self.workspaces.set_last(self.tab, &id) {
            self.save_workspaces();
        }
        self.reload();
    }

    /// A workspace tab (or All) was clicked: it shows the session last open there.
    fn switch_tab(&mut self, t: Option<usize>) {
        if self.tab == t {
            return;
        }
        self.tab = t;
        self.open_tab();
    }

    /// Opens the session last open in the current tab (or its newest; nothing if empty).
    fn open_tab(&mut self) {
        let t = self.tab;
        let visible = self.visible();
        let ids: Vec<&str> = visible.iter().map(|&i| self.sessions[i].id.as_str()).collect();
        match workspaces::pick_session(self.workspaces.last(t), &ids) {
            Some(k) => self.select(visible[k]),
            None => {
                self.switch_view(None);
                self.selected = None;
                self.open = None;
                self.new_session = None;
                self.history = History::default();
                self.pinned = None;
                self.inline_you = false;
            }
        }
    }

    /// Folder new sessions in the open workspace go to without asking: the one picked for it,
    /// else that of its open session. None in All sessions, or before the first is made.
    fn workspace_folder(&self) -> Option<PathBuf> {
        let w = self.workspaces.list.get(self.tab?)?;
        let open = self.open_id().filter(|id| w.sessions.contains(id)).and_then(|_| self.open.as_ref()?.cwd.clone());
        w.folder.clone().into_iter().chain(open).find(|d| d.is_dir())
    }

    /// Asks for a folder (starting at the last one chosen) and opens a new session there.
    fn choose_new_session_folder(&mut self) {
        let start = self
            .workspaces
            .last_folder
            .clone()
            .filter(|d| d.is_dir())
            .or_else(|| Some(PathBuf::from(self.settings.default_folder.trim())).filter(|d| d.is_dir()))
            .or_else(|| self.open.as_ref().and_then(|s| s.cwd.clone()));
        if let Some(dir) = runner::pick_folder(start) {
            self.workspaces.chose_folder(self.tab, &dir);
            self.save_workspaces();
            self.start_new_session(dir);
        }
    }

    /// Empty session in `dir`; the first prompt creates it.
    fn start_new_session(&mut self, dir: PathBuf) {
        self.switch_view(Some(format!("new:{}", dir.display())));
        self.selected = None;
        self.new_id = None;
        self.open = Some(Session { cwd: Some(dir.clone()), ..Default::default() });
        self.new_session = Some(dir);
        self.history = History::default();
        self.pinned = None;
        self.inline_you = false;
        self.error = None;
        self.focus_prompt = true;
    }

    fn open_pending(&mut self, id: &str) {
        let Some(run) = self.runs.get(id) else { return };
        let (cwd, prompt) = (run.cwd.clone(), run.prompt.clone());
        self.switch_view(Some(id.to_owned()));
        self.selected = None;
        self.new_id = Some(id.to_owned());
        let mut s = Session { cwd: cwd.clone(), ..Default::default() };
        s.push_prompt(prompt);
        self.open = Some(s);
        self.new_session = cwd.or_else(|| Some(PathBuf::from(".")));
        self.pinned = None;
        self.inline_you = false;
        self.heights.clear();
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
                self.inline_you = false;
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
        // A new session being looked at now exists on disk: it becomes a normal, listed one.
        if let (Some(_), Some(id)) = (&self.new_session, &self.new_id)
            && let Some(i) = self.sessions.iter().position(|s| &s.id == id)
        {
            self.selected = Some(i);
            self.new_session = None;
            if self.workspaces.set_last(self.tab, &self.sessions[i].id.clone()) {
                self.save_workspaces();
            }
            if !self.open_running() {
                self.reload();
            }
            return;
        }
        if changed && !self.open_running() {
            self.reload();
        }
    }

    /// Enter in the prompt input: sends it, or queues it while this session is working.
    fn send(&mut self) {
        let Some(cwd) = self.open.as_ref().map(|s| s.cwd.clone()) else { return };
        let prompt = std::mem::take(&mut self.input);
        if prompt.trim().is_empty() {
            return;
        }
        self.refine_undo = None;
        if let Some(id) = self.open_id().filter(|id| self.runs.contains_key(id)) {
            self.queues.entry(id).or_default().push(prompt);
            return;
        }
        let (id, new) = match (&self.new_session, self.selected) {
            (Some(_), _) => (runner::new_session_id(), true),
            (None, Some(i)) => (self.sessions[i].id.clone(), false),
            _ => return,
        };
        if new {
            self.new_id = Some(id.clone());
            if let Some(w) = self.tab {
                self.workspaces.move_to(&id, w);
                self.save_workspaces();
            }
        }
        self.start_run(id, cwd, prompt, new);
    }

    fn start_run(&mut self, id: String, cwd: Option<PathBuf>, prompt: String, new: bool) {
        self.interrupted.remove(&id);
        if self.open_id().as_ref() == Some(&id)
            && let Some(s) = &mut self.open
        {
            s.push_prompt(prompt.clone());
            self.history = History::new(s.prompts.clone());
        }
        let (rx, stop) = runner::send(self.ctx.clone(), id.clone(), cwd.clone(), prompt.clone(), new);
        self.runs.insert(id, Run { rx, live: String::new(), tool: None, new, cwd, prompt, stop, poked: false });
    }

    /// Ctrl+C / Stop: ends the open session's run at once.
    fn stop_open(&mut self) {
        if let Some(id) = self.open_id().filter(|id| self.runs.contains_key(id)) {
            self.end_run(&id, Err("stopped".into()), true);
        }
    }

    /// Id of the session in the main area: the selected one, or the new one being created.
    fn open_id(&self) -> Option<String> {
        match &self.new_session {
            Some(_) => self.new_id.clone(),
            None => self.selected.map(|i| self.sessions[i].id.clone()),
        }
    }

    fn open_running(&self) -> bool {
        self.open_id().is_some_and(|id| self.runs.contains_key(&id))
    }

    /// Drains every run's events. Steps of the open session's run are applied to the
    /// conversation as they arrive; other sessions catch up from disk when opened.
    fn poll_runner(&mut self) {
        let open_id = self.open_id();
        let mut finished = Vec::new();
        let mut poke = false;
        for (id, run) in &mut self.runs {
            let is_open = open_id.as_ref() == Some(id);
            for event in run.rx.try_iter() {
                match event {
                    RunEvent::Line(v) => {
                        if let Some(s) = self.open.as_mut().filter(|_| is_open) {
                            s.apply(&v);
                        }
                        run.live.clear();
                        run.tool = None;
                        // The new session's file exists now: list it without waiting.
                        if run.new && !run.poked {
                            run.poked = true;
                            poke = true;
                        }
                    }
                    RunEvent::Delta(t) => run.live.push_str(&t),
                    RunEvent::Tool(name) => run.tool = Some(name),
                    RunEvent::Done(result) => finished.push((id.clone(), result)),
                }
            }
        }
        if poke {
            let _ = self.poke_index.send(());
        }
        for (id, result) in finished {
            self.end_run(&id, result, false);
        }
    }

    /// A run ended (or was stopped): send the next queued prompt, or pause the queue and say why.
    fn end_run(&mut self, id: &str, result: Result<(), String>, stopped: bool) {
        let Some(run) = self.runs.remove(id) else { return };
        if stopped {
            run.stop.stop();
        }
        let is_open = self.open_id().as_deref() == Some(id);
        let name = self.sessions.iter().find(|s| s.id == id).map(|s| self.names.get(&s.id).unwrap_or(&s.title).to_owned());
        let name = name.unwrap_or_else(|| run.prompt.chars().take(40).collect());
        let queue = self.queues.entry(id.to_owned()).or_default();
        if stopped {
            queue.pause("stopped with Ctrl+C");
            self.interrupted.insert(id.to_owned(), run.live.clone());
            // The stopped prompt goes back to the input to be fixed (unless something is typed).
            if is_open && self.input.trim().is_empty() {
                self.input = run.prompt.clone();
                self.focus_prompt = true;
            } else if !is_open && let Some(v) = self.views.0.get_mut(id).filter(|v| v.draft.is_empty()) {
                v.draft = run.prompt.clone();
            }
        } else if let Err(e) = &result {
            queue.pause("the last run failed");
            self.error = Some(if is_open { e.clone() } else { format!("{name}: {e}") });
        } else if let Some(next) = queue.next() {
            self.start_run(id.to_owned(), run.cwd.clone(), next, false);
        }
        if self.settings.notify_when_done && !stopped && !self.ctx.input(|i| i.focused) {
            let body = match &result {
                Ok(()) => format!("{name}: Claude finished"),
                Err(e) => format!("{name}: {e}"),
            };
            runner::notify("sessman", &body);
        }
        // The file now holds what was streamed; reloading makes it canonical. A new session is
        // selected when the indexer lists it.
        if is_open && self.new_session.is_none() && !self.runs.contains_key(id) {
            self.reload();
        }
        let _ = self.poke_index.send(());
    }

    fn poll_ai(&mut self) {
        let Some(ai) = &mut self.ai else { return };
        let Some(result) = ai.rx.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
        ai.rx = None;
        ai.result = Some(result.map(|reply| {
            search::parse_ai(&reply, ai.ids.len()).into_iter().map(|(i, why)| (ai.ids[i].clone(), why)).collect()
        }));
    }

    /// Asks the helper model to refine the input (first round), or to write the prompt with
    /// the answers to its questions (second round; None = skip them all).
    fn start_refine(&mut self, answers: Option<Vec<(refine::Question, Option<String>)>>) {
        let original = match &self.refine {
            Some(r) if answers.is_some() => r.original.clone(),
            _ => self.input.clone(),
        };
        if original.trim().is_empty() {
            return;
        }
        let id = self.open_id();
        let goal = id.as_deref().and_then(|id| self.goals.get(id));
        let recent = self.open.as_ref().map(|s| s.prompts.clone()).unwrap_or_default();
        let request = refine::request(&original, goal, &recent, answers.as_deref());
        let rx = runner::ask_haiku(self.ctx.clone(), request);
        self.refine = Some(Refining { original, rx: Some(rx), questions: Vec::new(), error: None });
    }

    fn poll_refine(&mut self) {
        let Some(r) = &mut self.refine else { return };
        let Some(result) = r.rx.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
        r.rx = None;
        match result.and_then(|reply| refine::parse(&reply)) {
            Ok(refine::Reply::Prompt(p)) => {
                self.refine_undo = Some((r.original.clone(), p.clone()));
                self.input = p;
                self.refine = None;
                self.focus_prompt = true;
            }
            Ok(refine::Reply::Questions(qs)) => r.questions = qs.into_iter().map(|q| (q, None, String::new())).collect(),
            Err(e) => r.error = Some(e),
        }
    }

    /// The mic button: starts recording, or stops it and sends the audio off for transcription.
    fn toggle_dictation(&mut self) {
        match std::mem::replace(&mut self.dictation, Dictation::Idle) {
            Dictation::Recording(recorder) => {
                let audio = recorder.finish();
                let (tx, rx) = mpsc::channel();
                let ctx = self.ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(dictation::transcribe(&audio));
                    ctx.request_repaint();
                });
                self.dictation = Dictation::Transcribing(rx);
            }
            Dictation::Idle => {
                let model = dictation::model_path();
                if !model.exists() {
                    self.model_prompt.get_or_insert(ModelPrompt::Ask);
                    return;
                }
                match dictation::Recorder::start() {
                    Ok(recorder) => {
                        self.error = None;
                        self.mic_levels = std::iter::repeat_n(0.0, MIC_BARS).collect();
                        self.dictation = Dictation::Recording(recorder);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            busy @ Dictation::Transcribing(_) => self.dictation = busy,
        }
    }

    fn poll_dictation(&mut self) {
        let Dictation::Transcribing(rx) = &self.dictation else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("transcription stopped unexpectedly".to_owned()),
        };
        self.dictation = Dictation::Idle;
        match result {
            Ok(text) => self.insert_dictated(&text),
            Err(e) => self.error = Some(e),
        }
    }

    /// Puts dictated text into the input at the cursor, with a space before it if needed.
    fn insert_dictated(&mut self, text: &str) {
        let id = Id::new("prompt-input");
        let chars = self.input.chars().count();
        let at = egui::TextEdit::load_state(&self.ctx, id).and_then(|s| s.cursor.char_range()).map_or(chars, |r| usize::from(r.primary.index).min(chars));
        let (input, end) = dictation::insert_at(&self.input, at, text);
        if input == self.input {
            return;
        }
        self.input = input;
        // The cursor goes after the new text, once the input has been redrawn.
        let mut state = egui::TextEdit::load_state(&self.ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(end))));
        state.store(&self.ctx, id);
        self.focus_prompt = true;
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

    /// The search field at the bottom of the sidebar, with the AI search status above it, and
    /// the Settings button next to it.
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
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.search)
                .id(Id::new("search"))
                .hint_text("🔍 Search · Enter asks Haiku")
                .desired_width(ui.available_width() - 34.0);
            let resp = ui.add(edit);
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.search.clear();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.ask_ai(ui.ctx());
                resp.request_focus();
            }
            let cmd = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
            if ui.button("⚙").on_hover_text(format!("Settings ({cmd}+,)")).clicked() {
                self.open_settings();
            }
        });
    }

    fn open_settings(&mut self) {
        self.settings_tab = true;
        self.settings_active = true;
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let mut actions = Vec::new();
        self.tabs(ui, &mut actions);
        ui.add_space(4.0);
        let new_button = egui::Button::new(RichText::new("+  New session").strong()).min_size(egui::vec2(ui.available_width(), 28.0));
        let new_button = ui.add(new_button).on_hover_text(match self.workspace_folder() {
            Some(dir) => format!("In {}\nRight-click to choose another folder", dir.display()),
            None => "Choose a folder for the new session".to_owned(),
        });
        let mut choose = false;
        new_button.context_menu(|ui| {
            if ui.button("Choose folder…").clicked() {
                choose = true;
                ui.close();
            }
        });
        if new_button.clicked() {
            match self.workspace_folder() {
                Some(dir) => self.start_new_session(dir),
                None => choose = true,
            }
        }
        if choose {
            self.choose_new_session_folder();
        }
        ui.separator();
        egui::Panel::bottom("search")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
            .resizable(false)
            .show(ui, |ui| self.search_bar(ui));

        let rows = self.rows();
        let members = self.tab.and_then(|w| self.workspaces.list.get(w)).map(|w| w.sessions.as_slice());
        let pending = pending_new(&self.runs, &self.sessions, members);
        if rows.is_empty() && pending.is_empty() && !self.search.trim().is_empty() {
            ui.label(RichText::new("No matches. Press Enter to ask Haiku.").weak());
        } else if rows.is_empty() && pending.is_empty() {
            ui.label(if self.tab.is_some() {
                "Empty workspace. Drag sessions onto its tab, or right-click a session and choose Move to."
            } else {
                "No sessions found in ~/.claude/projects"
            });
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (id, title, _) in pending {
                let open = self.new_session.is_some() && self.new_id.as_ref() == Some(&id);
                if session_bubble(ui, open, &title, None, true, None).clicked() && !open {
                    actions.push(Action::OpenPending(id));
                }
            }
            for (i, note) in rows {
                let s = &self.sessions[i];
                let title = self.names.get(&s.id).unwrap_or(&s.title);
                if let Some(r) = self.renaming.as_mut().filter(|r| r.id == s.id) {
                    rename_field(ui, r, &s.project, &mut actions);
                    continue;
                }
                let queued = match self.queues.get(&s.id).map(|q| q.items.len()) {
                    Some(n) if n > 0 => format!("  +{n} queued"),
                    _ => String::new(),
                };
                let label = format!("{title}{queued}");
                let color = self.tab.is_none().then(|| self.workspaces.color_of(&s.id)).flatten();
                let row = session_bubble(ui, self.selected == Some(i), &label, note.as_deref(), self.runs.contains_key(&s.id), color);
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

    /// "Delete workspace?" when the setting asks for it.
    fn confirm_delete_modal(&mut self, ctx: &egui::Context) {
        let Some(w) = self.confirm_delete else { return };
        let name = self.workspaces.list.get(w).map(|w| w.name.clone()).unwrap_or_default();
        let (mut delete, mut cancel) = (false, false);
        let modal = egui::Modal::new(Id::new("confirm-delete")).show(ctx, |ui| {
            ui.label(RichText::new(format!("Delete workspace \"{name}\"?")).strong());
            ui.label("Its sessions are not deleted; they stay in All sessions.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                delete = ui.button("Delete").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if delete {
            self.delete_workspace(w);
        }
        if delete || cancel || modal.should_close() {
            self.confirm_delete = None;
        }
    }

    /// Offers to download the Whisper model that dictation needs, and shows the download.
    fn model_prompt_modal(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &mut self.model_prompt else { return };
        if let ModelPrompt::Downloading(child) = prompt {
            match child.try_wait() {
                Ok(None) => ctx.request_repaint_after(std::time::Duration::from_millis(250)),
                Ok(Some(status)) => {
                    let ModelPrompt::Downloading(child) = std::mem::replace(prompt, ModelPrompt::Done) else { unreachable!() };
                    if let Err(e) = dictation::finish_model_download(child, status) {
                        *prompt = ModelPrompt::Failed(e);
                    }
                }
                Err(e) => {
                    let _ = child.kill();
                    *prompt = ModelPrompt::Failed(format!("download failed: {e}"));
                }
            }
        }
        let mb = |bytes: u64| bytes as f32 / 1_000_000.0;
        let (mut download, mut cancel, mut close, mut dictate) = (false, false, false, false);
        let modal = egui::Modal::new(Id::new("model-prompt")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            match &*prompt {
                ModelPrompt::Ask => {
                    ui.label(RichText::new("Download the speech model?").strong());
                    ui.label(format!(
                        "Dictation transcribes on this Mac with the Whisper base model ({:.0} MB, from Hugging Face). Audio never leaves your machine.",
                        mb(dictation::MODEL_BYTES)
                    ));
                    ui.label(RichText::new(format!("It is saved to {}", dictation::model_path().display())).small().weak());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        download = ui.button("Download").clicked();
                        close = ui.button("Not now").clicked();
                    });
                }
                ModelPrompt::Downloading(_) => {
                    let got = std::fs::metadata(dictation::model_part_path()).map_or(0, |m| m.len());
                    let fraction = got as f32 / dictation::MODEL_BYTES as f32;
                    ui.label(RichText::new("Downloading the speech model…").strong());
                    ui.add(egui::ProgressBar::new(fraction.min(1.0)).text(format!("{:.0} of {:.0} MB", mb(got), mb(dictation::MODEL_BYTES))));
                    ui.add_space(8.0);
                    cancel = ui.button("Cancel").clicked();
                }
                ModelPrompt::Done => {
                    ui.label(RichText::new("Speech model installed").strong());
                    ui.label("Dictation is ready: click the mic to start, click again to stop.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        dictate = ui.button("Start dictating").clicked();
                        close = ui.button("Close").clicked();
                    });
                }
                ModelPrompt::Failed(e) => {
                    ui.label(RichText::new("Couldn't download the speech model").strong());
                    ui.label(e);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        download = ui.button("Retry").clicked();
                        close = ui.button("Close").clicked();
                    });
                }
            }
        });
        // Clicking outside or Esc closes it, except mid-download, where only Cancel stops it.
        let downloading = matches!(prompt, ModelPrompt::Downloading(_));
        if download {
            *prompt = match dictation::start_model_download() {
                Ok(child) => ModelPrompt::Downloading(child),
                Err(e) => ModelPrompt::Failed(e),
            };
        } else if cancel {
            if let ModelPrompt::Downloading(child) = prompt {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = std::fs::remove_file(dictation::model_part_path());
            self.model_prompt = None;
        } else if close || dictate || (!downloading && modal.should_close()) {
            self.model_prompt = None;
            if dictate {
                self.toggle_dictation();
            }
        }
    }

    /// Prompts waiting for the open session's run, right above the input.
    fn queue_list(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.open_id() else { return };
        let Some(q) = self.queues.get(&id).filter(|q| !q.is_empty()) else { return };
        let pal = theme::palette(ui.visuals());
        let (mut remove, mut edit, mut resume) = (None, None, false);
        egui::Frame::new().stroke(Stroke::new(1.0, pal.border)).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if q.paused {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("⏸ Queue paused: {}", q.reason.as_deref().unwrap_or("paused"))).color(pal.note));
                    resume = ui.small_button("Resume queue").clicked();
                });
            }
            for (i, p) in q.items.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("x").on_hover_text("Remove from the queue").clicked() {
                        remove = Some(i);
                    }
                    let first: String = p.lines().next().unwrap_or_default().chars().take(140).collect();
                    let more = if p.lines().count() > 1 || first.chars().count() < p.chars().count() { " …" } else { "" };
                    let text = RichText::new(format!("queued {} · {first}{more}", i + 1)).monospace().weak();
                    let row = ui.add(egui::Label::new(text).truncate().sense(Sense::click())).on_hover_text(format!("{p}\n\nClick to edit"));
                    if row.clicked() {
                        edit = Some(i);
                    }
                });
            }
        });
        ui.add_space(4.0);
        let q = self.queues.entry(id.clone()).or_default();
        if let Some(i) = remove {
            q.take(i);
        }
        let (paused, reason) = (q.paused, q.reason.clone());
        if let Some(p) = edit.and_then(|i| q.take(i)) {
            if !self.input.trim().is_empty() {
                // Keep what was typed: it takes the edited prompt's place in the queue,
                // which stays paused if it was.
                q.items.insert(0, std::mem::take(&mut self.input));
                q.paused = paused;
                q.reason = reason;
            }
            self.input = p;
            self.focus_prompt = true;
        }
        if resume && let Some(next) = q.resume() {
            if self.runs.contains_key(&id) {
                q.items.insert(0, next);
            } else {
                let cwd = self.open.as_ref().and_then(|s| s.cwd.clone());
                self.start_run(id, cwd, next, false);
            }
        }
    }

    /// Refine's status and questions, above the input.
    fn refine_panel(&mut self, ui: &mut egui::Ui) {
        let pal = theme::palette(ui.visuals());
        if let Some((original, refined)) = &self.refine_undo {
            if &self.input == refined {
                let cmd = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
                let mut undo = false;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✨ Refined").color(pal.note));
                    undo = ui.link("Undo refine").on_hover_text(format!("{cmd}+Z")).clicked();
                });
                if undo {
                    self.input = original.clone();
                    self.refine_undo = None;
                }
            } else {
                self.refine_undo = None;
            }
        }
        let Some(r) = &mut self.refine else { return };
        let (mut go, mut skip_all, mut cancel) = (false, false, false);
        egui::Frame::new().stroke(Stroke::new(1.0, pal.note)).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(e) = &r.error {
                ui.horizontal(|ui| {
                    ui.colored_label(pal.error, format!("Refine failed: {e}"));
                    cancel = ui.small_button("Dismiss").clicked();
                });
            } else if r.rx.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Refining the prompt… (Esc to cancel)");
                });
            } else {
                ui.label(RichText::new("A few questions first. Pick an answer, write your own, or skip.").strong());
                for (n, (q, answer, other)) in r.questions.iter_mut().enumerate() {
                    ui.add_space(6.0);
                    ui.label(&q.question);
                    ui.horizontal_wrapped(|ui| {
                        for opt in &q.options {
                            let chosen = answer.as_ref() == Some(opt);
                            if ui.selectable_label(chosen, opt).clicked() {
                                *answer = if chosen { None } else { Some(opt.clone()) };
                            }
                        }
                        let field = ui.add(egui::TextEdit::singleline(other).id(Id::new(("refine-other", n))).hint_text("Other…").desired_width(180.0));
                        if field.changed() {
                            *answer = Some(other.clone()).filter(|o| !o.trim().is_empty());
                        }
                        let skipped = answer.is_none();
                        if ui.selectable_label(skipped, "Skip").clicked() {
                            *answer = None;
                            other.clear();
                        }
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    go = ui.button("✨ Refine with these answers").clicked();
                    skip_all = ui.button("Skip all").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            }
        });
        ui.add_space(4.0);
        if go || skip_all {
            let answers = r.questions.iter().map(|(q, a, _)| (q.clone(), if skip_all { None } else { a.clone() })).collect();
            self.start_refine(Some(answers));
        }
        if cancel {
            self.refine = None;
        }
    }

    /// The drag handle on the input's top edge: drag to resize, double-click for automatic
    /// size, chevron to collapse to one line.
    fn prompt_handle(&mut self, ui: &mut egui::Ui) {
        let pal = theme::palette(ui.visuals());
        let width = ui.available_width();
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width - 22.0, 8.0), Sense::click_and_drag());
        let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeVertical).on_hover_text("Drag to resize · double-click: automatic size");
        let color = if resp.hovered() || resp.dragged() { pal.prompt_sign } else { pal.border };
        ui.painter().hline(egui::Rangef::new(rect.center().x - 20.0, rect.center().x + 20.0), rect.center().y, Stroke::new(2.0, color));
        if resp.dragged() {
            let base = self.prompt_height.unwrap_or(self.prompt_measured);
            self.prompt_height = Some(base - resp.drag_delta().y);
            self.prompt_collapsed = false;
        }
        if resp.double_clicked() {
            self.prompt_height = None;
            self.prompt_collapsed = false;
        }
        // A small triangle: down to collapse, up to expand.
        let (crect, toggle) = ui.allocate_exact_size(egui::vec2(18.0, 8.0), Sense::click());
        let toggle = toggle.on_hover_text(if self.prompt_collapsed { "Expand the input" } else { "Collapse the input to one line" });
        let c = crect.center();
        let (w, h) = (4.5, 3.0);
        let dir = if self.prompt_collapsed { -1.0 } else { 1.0 };
        let points = vec![c + egui::vec2(-w, -h * dir), c + egui::vec2(w, -h * dir), c + egui::vec2(0.0, h * dir)];
        let color = if toggle.hovered() { pal.prompt_sign } else { ui.visuals().weak_text_color() };
        ui.painter().add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
        if toggle.clicked() {
            self.prompt_collapsed = !self.prompt_collapsed;
        }
    }

    fn prompt_bar(&mut self, ui: &mut egui::Ui, area_height: f32) {
        let id = Id::new("prompt-input");
        let running = self.open_running();
        let enabled = self.open.is_some();
        let focused = ui.memory(|m| m.has_focus(id));
        // Send and refine keys are taken before the TextEdit sees them (it would add a line).
        let (send, refine_key) = if focused {
            let with_mod = self.settings.send_with_modifier;
            ui.input_mut(|i| {
                let enter = i.key_pressed(Key::Enter);
                let (cmd, shift) = (i.modifiers.command, i.modifiers.shift);
                let send = enter && if with_mod { cmd && !shift } else { !cmd && !shift };
                let refine = enter && if with_mod { cmd && shift } else { cmd && !shift };
                if send || refine {
                    i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::Enter, .. }));
                }
                (send, refine)
            })
        } else {
            (false, false)
        };
        // Cmd+Z right after a refine brings the original text back.
        if focused
            && let Some((original, refined)) = self.refine_undo.clone()
            && self.input == refined
            && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z))
        {
            self.input = original;
            self.refine_undo = None;
        }
        // Arrows drive history (and pull back queued prompts) instead of the cursor, only
        // for a one-line input, where there are no lines to move between.
        if focused && !self.input.contains('\n') {
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                let queued = self.open_id().and_then(|id| self.queues.get_mut(&id)).filter(|_| self.input.is_empty()).and_then(|q| q.pull_last());
                if let Some(p) = queued.or_else(|| self.history.up(&self.input)) {
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
        self.queue_list(ui);
        self.refine_panel(ui);
        ui.horizontal(|ui| self.prompt_handle(ui));
        let mut stop = false;
        let mut refine_click = false;
        let mut dictate_click = false;
        let recording = matches!(self.dictation, Dictation::Recording(_));
        let time = ui.input(|i| i.time);
        // While recording, the input's border turns red and breathes, about once a second.
        let border = if recording {
            let pulse = 0.5 + 0.5 * (time * std::f64::consts::TAU / 1.2).sin() as f32;
            Stroke::new(1.5, pal.error.gamma_multiply(0.35 + 0.65 * pulse))
        } else {
            Stroke::new(1.0, pal.border)
        };
        egui::Frame::new()
            .fill(pal.prompt_bg)
            .stroke(border)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(">").monospace().strong().color(pal.prompt_sign));
                    let send_key = if self.settings.send_with_modifier { "cmd+enter" } else { "enter" };
                    let hint = match (&self.open, running) {
                        _ if recording => "listening… click ⏹ to stop and transcribe".to_owned(),
                        (None, _) => "select a session".to_owned(),
                        (_, true) => format!("claude is working… {send_key} queues a prompt · ctrl+c stops"),
                        _ => format!("type a prompt · {send_key} to send · cmd+enter refines · up/down for history"),
                    };
                    let row = ui.text_style_height(&egui::TextStyle::Monospace);
                    let (min, max) = input_heights(row, self.settings.input_max_lines, self.prompt_height, self.prompt_collapsed, area_height * 0.7);
                    // The panel only grows if asked: the text's full height (measured last
                    // frame), within min..max. The bottom edge stays put, so it grows upwards.
                    let height = self.prompt_text_height.clamp(min, max);
                    // Room for the buttons on the right: refine, stop, and the mic with its spinner.
                    let buttons = (if running { 84.0 } else { 40.0 })
                        + if dictation::AVAILABLE { 48.0 } else { 0.0 }
                        + match self.dictation {
                            Dictation::Recording(_) => MIC_METER_WIDTH,
                            Dictation::Transcribing(_) => 90.0,
                            Dictation::Idle => 0.0,
                        };
                    let width = ui.available_width() - buttons;
                    let out = egui::ScrollArea::vertical()
                        .id_salt("prompt-scroll")
                        .min_scrolled_height(height)
                        .max_height(height)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            let edit = egui::TextEdit::multiline(&mut self.input)
                                .id(id)
                                .frame(egui::Frame::NONE)
                                .font(egui::TextStyle::Monospace)
                                .text_color(pal.prompt_text)
                                .hint_text(RichText::new(hint).monospace())
                                .desired_rows(((min / row).floor() as usize).max(1))
                                .return_key(if self.settings.send_with_modifier {
                                    None
                                } else {
                                    Some(egui::KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                                })
                                .desired_width(width);
                            ui.add_enabled(enabled, edit)
                        });
                    self.prompt_measured = out.inner_rect.height();
                    if (out.content_size.y - self.prompt_text_height).abs() > 0.5 {
                        self.prompt_text_height = out.content_size.y;
                        ui.ctx().request_repaint();
                    }
                    let resp = out.inner;
                    if self.focus_prompt && enabled {
                        resp.request_focus();
                    }
                    self.focus_prompt = false;
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let waiting = self.refine.as_ref().is_some_and(|r| r.rx.is_some());
                        let cmd = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
                        let keys = if self.settings.send_with_modifier { format!("{cmd}+Shift+Enter") } else { format!("{cmd}+Enter") };
                        let refine = egui::Button::new("✨").frame(false);
                        refine_click = ui
                            .add_enabled(enabled && !waiting && !self.input.trim().is_empty(), refine)
                            .on_hover_text(format!("Refine the prompt ({keys})"))
                            .on_disabled_hover_text("Type a prompt to refine it")
                            .clicked();
                        // Added after refine, so it sits to the left of it.
                        if dictation::AVAILABLE {
                            let transcribing = matches!(self.dictation, Dictation::Transcribing(_));
                            let (icon, color) = if recording { ("⏹", pal.error) } else { ("🎤", ui.visuals().text_color()) };
                            let mic = egui::Button::new(RichText::new(icon).color(color)).frame(false);
                            let hover = if recording { "Stop dictation" } else { "Dictate: click to start, click again to stop" };
                            dictate_click = ui
                                .add_enabled(recording || (enabled && !transcribing), mic)
                                .on_hover_text(hover)
                                .on_disabled_hover_text(if transcribing { "Transcribing…" } else { "Select a session first" })
                                .clicked();
                            if transcribing {
                                ui.spinner();
                                ui.label(RichText::new("transcribing…").small().weak());
                            }
                            if let Dictation::Recording(recorder) = &self.dictation {
                                recording_meter(ui, recorder, &mut self.mic_levels, pal.error);
                            }
                        }
                        if running {
                            stop = ui.button("⏹").on_hover_text("Stop Claude (Ctrl+C)").clicked();
                            ui.spinner();
                        }
                    });
                });
            });
        if stop {
            self.stop_open();
        }
        if dictate_click {
            self.toggle_dictation();
        }
        if enabled && (refine_key || refine_click) && self.refine.as_ref().is_none_or(|r| r.rx.is_none()) {
            self.start_refine(None);
        }
        if enabled && send {
            self.send();
        }
    }

    fn conversation(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.open else {
            ui.centered_and_justified(|ui| ui.label(if self.tab.is_some() && self.visible().is_empty() {
                "This workspace is empty"
            } else {
                "Select a session on the left"
            }));
            return;
        };
        if let Some(dir) = self.new_session.as_ref().filter(|_| s.messages.is_empty()) {
            ui.centered_and_justified(|ui| {
                let status = if self.open_running() { "Starting…" } else { "Type your first prompt below." };
                ui.label(format!("New session in {}\n\n{status}", dir.display()));
            });
            return;
        }
        let pal = theme::palette(ui.visuals());
        // Pinned "You": the prompt of the turn at the top of the viewport (computed last frame),
        // hidden while a "You" prompt is on screen in the conversation so it never shows twice.
        let pinned = self.pinned.or_else(|| s.messages.iter().position(|m| pinned_text(m).is_some()));
        if let Some(text) = pinned.filter(|_| !self.inline_you).and_then(|i| s.messages.get(i)).and_then(pinned_text) {
            egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(6.0).corner_radius(4.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("You").strong().color(pal.user));
                egui::ScrollArea::vertical()
                    .id_salt("pinned")
                    .max_height(90.0)
                    .show(ui, |ui| ui.push_id("pinned", |ui| markdown::show(ui, &mut self.md_cache, &text)));
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
        let mut inline_you = false;
        let open_id = self.open_id();
        let key = self.view_key();
        let mut find = self.find.open.then_some(&mut self.find);
        egui::ScrollArea::vertical().id_salt(("conversation", &key)).auto_shrink(false).stick_to_bottom(true).show_viewport(ui, |ui, viewport| {
            let origin = ui.cursor().top();
            // Render only messages near the viewport; the rest just take their remembered height.
            let near = viewport.expand2(egui::vec2(0.0, 400.0));
            for (i, m) in s.messages.iter().enumerate() {
                let y = ui.cursor().top() - origin;
                if pinned_text(m).is_some() && y <= viewport.min.y + 1.0 {
                    top_prompt = Some(i);
                }
                let h = self.heights[i];
                let skip = h > 0.0 && (y + h < near.min.y || y > near.max.y);
                let from = find::next_shape(ui);
                let rect = if skip {
                    ui.allocate_space(egui::vec2(ui.available_width(), h)).1
                } else {
                    let rect = ui.push_id(i, |ui| message(ui, &mut self.md_cache, m, &pal)).response.rect;
                    self.heights[i] = rect.height().max(1.0);
                    rect
                };
                if let Some(f) = find.as_deref_mut().filter(|f| f.matches.iter().any(|m| m.item == i)) {
                    let (_, current) = f.in_item(i);
                    let exact = if skip { None } else { find::highlight_painted(ui, from, &f.query, current, pal.find) };
                    if current.is_some() {
                        // The current match's message is outlined too, in case it is in something not shown (a link).
                        ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.5, pal.find.current_bg), StrokeKind::Outside);
                    }
                    scroll_to_match(ui, f, i, rect, m.text.len(), exact, !skip);
                }
                if pinned_text(m).is_some() && y < viewport.max.y && y + self.heights[i] > viewport.min.y {
                    inline_you = true;
                }
            }
            if let Some(run) = open_id.as_ref().and_then(|id| self.runs.get(id)) {
                // The reply as it is being written, then what Claude is doing right now.
                if !run.live.is_empty() {
                    let m = session::Message { role: Role::Assistant, text: run.live.clone() };
                    ui.push_id("live", |ui| message(ui, &mut self.md_cache, &m, &pal));
                }
                ui.horizontal(|ui| {
                    ui.spinner();
                    let doing = match &run.tool {
                        Some(tool) => format!("⚙ {tool}…"),
                        None if run.live.is_empty() => "Claude is working…".to_owned(),
                        None => "Writing…".to_owned(),
                    };
                    ui.label(RichText::new(doing).weak().monospace());
                });
            } else if let Some(partial) = open_id.as_ref().and_then(|id| self.interrupted.get(id)).filter(|_| !saved_interrupt(&s.messages)) {
                // What Claude had written when it was stopped, then the mark.
                if !partial.is_empty() {
                    let m = session::Message { role: Role::Assistant, text: partial.clone() };
                    ui.push_id("interrupted", |ui| message(ui, &mut self.md_cache, &m, &pal));
                }
                ui.label(RichText::new("⏹ Interrupted").color(pal.note).monospace());
            }
        });
        if top_prompt != self.pinned || inline_you != self.inline_you {
            self.pinned = top_prompt;
            self.inline_you = inline_you;
            ui.ctx().request_repaint();
        }
    }

    fn file_options(&self) -> diffview::FileOptions {
        diffview::FileOptions {
            line_numbers: self.settings.line_numbers,
            wrap: self.settings.wrap_file_lines,
            tab_width: self.settings.tab_width,
            font_size: self.settings.mono_size + 0.5,
        }
    }

    /// A file tab: Markdown files render, with a toggle to their source; others show as source.
    fn file_tab(&mut self, ui: &mut egui::Ui, i: usize) {
        let opts = self.file_options();
        let scroll_id = ("file", self.view_key(), self.file_tabs[i].path.clone());
        let tab = &mut self.file_tabs[i];
        let find = self.find.open.then_some(&mut self.find);
        let text = match (&tab.contents, markdown::is_markdown(&tab.path)) {
            (Ok(text), true) => text,
            _ => return diffview::file_view(ui, &tab.path, &tab.contents, &opts, scroll_id, find),
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab.rendered, true, "Rendered");
            ui.selectable_value(&mut tab.rendered, false, "Source");
        });
        if !tab.rendered {
            return diffview::file_view(ui, &tab.path, &tab.contents, &opts, scroll_id, find);
        }
        ui.label(RichText::new(&tab.path).weak().monospace());
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt(("md", scroll_id)).auto_shrink(false).show(ui, |ui| {
            let from = find::next_shape(ui);
            let rect = egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| markdown::show(ui, &mut self.md_cache, text)).response.rect;
            if let Some(f) = find {
                let exact = find::highlight_painted(ui, from, &f.query, f.in_item(0).1, theme::palette(ui.visuals()).find);
                scroll_to_match(ui, f, 0, rect, text.len(), exact, true);
            }
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let status_line = match status::setup() {
            status::Setup::Enabled => "Enabled".to_owned(),
            status::Setup::NotSet => "Not enabled (Enable it in the Info tab, under Usage limits)".to_owned(),
            status::Setup::Other(cmd) => format!("Your own status line is set (`{cmd}`)"),
        };
        let about = settings::About { status_line: &status_line };
        if settings::ui(ui, &mut self.settings, &mut self.settings_filter, &about) {
            self.apply_settings();
            if let Err(e) = self.settings.save(&settings::path()) {
                self.error = Some(format!("could not save settings: {e}"));
            }
        }
    }

    /// Zoom is changed with Cmd +/- (no slider); keep the one in use so the next launch has it.
    fn keep_zoom(&mut self, ctx: &egui::Context) {
        let zoom = ctx.zoom_factor();
        if (zoom - self.settings.zoom).abs() < 0.001 {
            return;
        }
        self.settings.zoom = zoom;
        if let Err(e) = self.settings.save(&settings::path()) {
            self.error = Some(format!("could not save settings: {e}"));
        }
    }

    fn right_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.right_tab, RightTab::Changes, RichText::new("Changes").heading());
            ui.selectable_value(&mut self.right_tab, RightTab::Info, RichText::new("Info").heading());
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
                let rendered = markdown::is_markdown(path) && self.settings.markdown_rendered;
                self.file_tabs.push(FileTab { path: path.to_owned(), contents: diffview::read(path), rendered });
                self.file_tabs.len() - 1
            }
        };
        self.main_tab = Some(i);
        self.settings_active = false;
    }

    fn close_file(&mut self, i: usize) {
        drop(self.file_tabs.remove(i));
        self.main_tab = after_close(self.main_tab, i);
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

    /// "Main" plus one tab per open file (and Settings), above the main area.
    /// Searches what the main area shows: the open file tab, or the conversation's messages
    /// (tool-only ones only while tools are shown).
    fn update_find(&mut self) {
        if !self.find.open {
            return;
        }
        if let Some(t) = self.main_tab.and_then(|i| self.file_tabs.get(i)) {
            let Ok(text) = &t.contents else { return self.find.update(&t.path, &[]) };
            // Files show tab-expanded (match ranges index what is drawn); rendered Markdown, its source.
            if markdown::is_markdown(&t.path) && t.rendered {
                return self.find.update(&format!("md:{}", t.path), &[text]);
            }
            let text = diffview::expand_tabs(text, self.settings.tab_width);
            return self.find.update(&format!("file:{}", t.path), &[&text]);
        }
        let show_tools = markdown::SHOW_TOOLS.load(std::sync::atomic::Ordering::Relaxed);
        let texts: Vec<&str> = match &self.open {
            Some(s) => s
                .messages
                .iter()
                .map(|m| if !show_tools && m.role == Role::Assistant && m.text.lines().all(|l| l.starts_with("[tool:")) { "" } else { m.text.as_str() })
                .collect(),
            None => Vec::new(),
        };
        let target = format!("conv:{}", self.view_key().unwrap_or_default());
        self.find.update(&target, &texts);
    }

    /// The find bar at the top right of the main area: the query, "3 of 17", previous/next and
    /// close. Enter / Shift+Enter step through the matches; Esc or a click elsewhere hides it (in `ui`).
    fn find_bar(&mut self, ui: &mut egui::Ui) {
        // Right to left, so the widgets are added from the close button back to the query.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("✕").on_hover_text("Close (Esc)").clicked() {
                self.find.close();
                return;
            }
            let some = !self.find.matches.is_empty();
            if ui.add_enabled(some, egui::Button::new("⏷").small()).on_hover_text("Next (Enter)").clicked() {
                self.find.step(true);
            }
            if ui.add_enabled(some, egui::Button::new("⏶").small()).on_hover_text("Previous (Shift+Enter)").clicked() {
                self.find.step(false);
            }
            ui.label(RichText::new(self.find.status()).weak());
            let id = Id::new("find-input");
            let mut out = egui::TextEdit::singleline(&mut self.find.query).id(id).hint_text("Find").desired_width(260.0).show(ui);
            if self.find.focus {
                self.find.focus = false;
                out.response.request_focus();
                let all = CCursorRange::two(CCursor::new(0), CCursor::new(self.find.query.chars().count()));
                out.state.cursor.set_char_range(Some(all));
                out.state.store(ui.ctx(), id);
            }
            let (enter, shift, esc) = ui.input(|i| (i.key_pressed(Key::Enter), i.modifiers.shift, i.key_pressed(Key::Escape)));
            if out.response.lost_focus() && enter && !esc {
                self.find.step(!shift);
                out.response.request_focus();
            }
            // The count is drawn before the field; show the new one on the next frame.
            if out.response.changed() || enter {
                ui.ctx().request_repaint();
            }
        });
    }

    fn main_tabs(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
        let mut close_settings = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let main_active = self.main_tab.is_none() && !self.settings_active;
            if ui.selectable_label(main_active, RichText::new("Main").strong()).clicked() {
                self.main_tab = None;
                self.settings_active = false;
            }
            for (i, t) in self.file_tabs.iter().enumerate() {
                let (name, _) = diffview::split_path(&t.path);
                let active = self.main_tab == Some(i) && !self.settings_active;
                match closable_tab(ui, ("file-tab", i), RichText::new(name).monospace(), active, &t.path) {
                    TabClick::Select => {
                        self.main_tab = Some(i);
                        self.settings_active = false;
                    }
                    TabClick::Close => close = Some(i),
                    TabClick::None => {}
                }
            }
            if self.settings_tab {
                match closable_tab(ui, "settings-tab", RichText::new("⚙ Settings"), self.settings_active, "Settings") {
                    TabClick::Select => self.settings_active = true,
                    TabClick::Close => close_settings = true,
                    TabClick::None => {}
                }
            }
        });
        if let Some(i) = close {
            self.close_file(i);
        }
        if close_settings {
            self.settings_tab = false;
            self.settings_active = false;
        }
    }

    /// The bar along the top of the window, under the macOS window buttons: drag it to move
    /// the window, double-click to zoom.
    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let resp = ui.interact(rect, Id::new("title-bar"), Sense::click_and_drag());
        if resp.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            let zoomed = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!zoomed));
        }
    }

    /// Ctrl+C (stop) and Cmd+, (settings), wherever the focus is.
    fn global_keys(&mut self, ctx: &egui::Context) {
        let ctrl_c = ctx.input_mut(|i| {
            // On macOS Ctrl+C is free (copy is Cmd+C); elsewhere it stops only with nothing to copy.
            let pressed = i.modifiers.ctrl && !i.modifiers.mac_cmd && !i.modifiers.shift && i.key_pressed(Key::C);
            if pressed {
                i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::C, .. } | egui::Event::Copy));
            }
            pressed
        });
        if ctrl_c && (cfg!(target_os = "macos") || self.input.is_empty()) {
            self.stop_open();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Comma)) {
            self.open_settings();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F) || i.consume_key(Modifiers::COMMAND, Key::F)) {
            self.find.show();
        }
        // Esc cancels a refine that is still waiting (egui also uses Esc to drop focus, so
        // this can't depend on the input being focused).
        if self.refine.as_ref().is_some_and(|r| r.rx.is_some()) && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.refine = None;
            self.focus_prompt = true;
        }
    }
}

/// Scrolls to the current find match once, if it is in `item`, drawn as `rect` from a text
/// `len` bytes long: to `exact` (where it was painted) when known, or else to a spot estimated
/// from how far into the text it is. An item that wasn't `drawn` (off screen) keeps the scroll
/// pending, so the next frame, with it drawn, lands exactly.
fn scroll_to_match(ui: &egui::Ui, f: &mut Find, item: usize, rect: egui::Rect, len: usize, exact: Option<egui::Rect>, drawn: bool) {
    let Some(start) = f.current_match().filter(|m| f.scroll && m.item == item).map(|m| m.range.start) else { return };
    f.scroll = !drawn;
    let target = exact.map(|r| r.expand(20.0)).unwrap_or_else(|| {
        let y = rect.top() + rect.height() * start as f32 / len.max(1) as f32;
        egui::Rect::from_x_y_ranges(rect.x_range(), y - 20.0..=y + 20.0)
    });
    ui.scroll_to_rect(target, Some(egui::Align::Center));
}

/// Claude Code's own record of a stopped run, saved as a user message.
fn is_interrupt_marker(text: &str) -> bool {
    text.contains("[Request interrupted by user")
}

/// What the pinned "You" shows for a message: a prompt the user typed, without Claude's stop
/// marker; None for Claude's messages and for a marker on its own.
fn pinned_text(m: &session::Message) -> Option<String> {
    if m.role != Role::User {
        return None;
    }
    let (before, after) = split_interrupt_marker(&m.text);
    let text = if after.is_empty() { before } else { after };
    (!text.is_empty()).then_some(text)
}

/// The text before and after Claude's stop marker (`[Request interrupted by user…]`).
fn split_interrupt_marker(text: &str) -> (String, String) {
    let Some(start) = text.find("[Request interrupted by user") else { return (text.trim().to_owned(), String::new()) };
    let end = text[start..].find(']').map_or(text.len(), |e| start + e + 1);
    (text[..start].trim().to_owned(), text[end..].trim().to_owned())
}

/// True once the session file holds the stop (Claude saved the partial reply and its marker),
/// so sessman's own copy of the partial reply isn't shown twice.
fn saved_interrupt(messages: &[session::Message]) -> bool {
    messages.iter().rev().take_while(|m| !(m.role == Role::User && !is_interrupt_marker(&m.text))).any(|m| is_interrupt_marker(&m.text))
}

/// The input's (min, max) height: one row when collapsed, the dragged height, or growing with
/// the text up to `max_lines`; never above `limit`.
fn input_heights(row: f32, max_lines: usize, manual: Option<f32>, collapsed: bool, limit: f32) -> (f32, f32) {
    let limit = limit.max(row);
    match (collapsed, manual) {
        (true, _) => (row, row),
        (false, Some(h)) => {
            let h = h.clamp(row, limit);
            (h, h)
        }
        (false, None) => (row, (row * max_lines.max(1) as f32).min(limit)),
    }
}

/// Active file tab after closing tab `closed`.
fn after_close(active: Option<usize>, closed: usize) -> Option<usize> {
    match active {
        Some(t) if t == closed => None,
        Some(t) if t > closed => Some(t - 1),
        t => t,
    }
}

enum TabClick {
    None,
    Select,
    Close,
}

/// A tab with its close button inside it: one shape that highlights as a whole, with the `x`
/// getting its own hover. Middle-click closes too.
fn closable_tab(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, text: RichText, selected: bool, hover: &str) -> TabClick {
    let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button);
    let pad = egui::vec2(8.0, 4.0);
    let x_size = 16.0;
    let size = egui::vec2(pad.x + galley.size().x + 6.0 + x_size + 4.0, galley.size().y.max(x_size) + 2.0 * pad.y);
    let (rect, tab) = ui.allocate_exact_size(size, Sense::click());
    let x_rect = egui::Rect::from_center_size(egui::pos2(rect.right() - 4.0 - x_size / 2.0, rect.center().y), egui::vec2(x_size, x_size));
    let x = ui.interact(x_rect, ui.id().with(&id).with("close"), Sense::click()).on_hover_text("Close");
    let tab = tab.on_hover_text(hover);
    let v = ui.visuals();
    let fill = if selected {
        v.selection.bg_fill
    } else if tab.hovered() || x.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        v.widgets.inactive.weak_bg_fill
    };
    let painter = ui.painter();
    painter.rect(rect, 4.0, fill, Stroke::new(1.0, if selected { v.selection.stroke.color } else { v.widgets.noninteractive.bg_stroke.color }), StrokeKind::Inside);
    let text_color = if selected { v.selection.stroke.color } else { v.text_color() };
    painter.galley(egui::pos2(rect.left() + pad.x, rect.center().y - galley.size().y / 2.0), galley, text_color);
    if x.hovered() {
        painter.rect_filled(x_rect, 3.0, v.widgets.active.weak_bg_fill);
    }
    let x_color = if x.hovered() || tab.hovered() || selected { text_color } else { v.weak_text_color() };
    let c = x_rect.center();
    let d = 3.5;
    painter.line_segment([c + egui::vec2(-d, -d), c + egui::vec2(d, d)], Stroke::new(1.4, x_color));
    painter.line_segment([c + egui::vec2(-d, d), c + egui::vec2(d, -d)], Stroke::new(1.4, x_color));
    if x.clicked() || tab.middle_clicked() {
        TabClick::Close
    } else if tab.clicked() {
        TabClick::Select
    } else {
        TabClick::None
    }
}

/// New sessions whose first prompt is running, before Claude has written their file (so the
/// list doesn't have them yet): (id, first line of the prompt, folder). In a workspace tab,
/// only the ones created there.
fn pending_new(runs: &HashMap<String, Run>, listed: &[SessionMeta], members: Option<&[String]>) -> Vec<(String, String, String)> {
    let mut out: Vec<_> = runs
        .iter()
        .filter(|(id, r)| r.new && !listed.iter().any(|s| &s.id == *id))
        .filter(|(id, _)| members.is_none_or(|m| m.contains(id)))
        .map(|(id, r)| {
            let folder = r.cwd.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
            (id.clone(), r.prompt.lines().next().unwrap_or_default().chars().take(60).collect(), folder)
        })
        .collect();
    out.sort();
    out
}

/// "/Users/me/Desktop/proj" → "Desktop/proj"
/// A session in the list: its name in a rounded bubble, with a spinner on the right while
/// Claude is working in it, and a search note under the name when there is one. Senses click
/// and drag in one widget (a separate drag-source wrapper would swallow the click).
fn session_bubble(ui: &mut egui::Ui, selected: bool, title: &str, note: Option<&str>, running: bool, color: Option<[u8; 3]>) -> egui::Response {
    const PAD: egui::Vec2 = egui::vec2(10.0, 6.0);
    const SPIN: f32 = 12.0;
    let width = ui.available_width();
    let text_width = (width - 2.0 * PAD.x - if running { SPIN + 6.0 } else { 0.0 }).max(10.0);
    let mut job = egui::text::LayoutJob::simple(title.to_owned(), egui::TextStyle::Body.resolve(ui.style()), ui.visuals().text_color(), text_width);
    job.wrap.max_rows = 2;
    let galley = ui.painter().layout_job(job);
    let note = note.map(|n| ui.painter().layout(n.to_owned(), egui::TextStyle::Small.resolve(ui.style()), ui.visuals().weak_text_color(), text_width));
    let height = galley.size().y + note.as_ref().map_or(0.0, |n| n.size().y + 2.0) + 2.0 * PAD.y;
    let (rect, row) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click_and_drag());
    if ui.is_rect_visible(rect) {
        let v = ui.visuals();
        let fill = if selected {
            v.selection.bg_fill
        } else if row.hovered() {
            v.widgets.hovered.weak_bg_fill
        } else if let Some(c) = color {
            tint(c, 45)
        } else {
            v.faint_bg_color
        };
        let stroke = match color {
            Some(c) => Stroke::new(1.0, rgb(c)),
            None if selected => v.selection.stroke,
            None => v.widgets.noninteractive.bg_stroke,
        };
        let radius = 10.0;
        ui.painter().rect(rect, radius, fill, stroke, StrokeKind::Inside);
        let text_color = if selected { v.selection.stroke.color } else { v.text_color() };
        let mut pos = rect.min + PAD;
        let galley_height = galley.size().y;
        ui.painter().galley(pos, galley, text_color);
        if let Some(n) = note {
            pos.y += galley_height + 2.0;
            ui.painter().galley(pos, n, v.weak_text_color());
        }
        if running {
            let spin = egui::Rect::from_center_size(egui::pos2(rect.max.x - PAD.x - SPIN / 2.0, rect.center().y), egui::vec2(SPIN, SPIN));
            egui::Spinner::new().size(SPIN).paint_at(ui, spin);
        }
    }
    ui.add_space(2.0);
    if running { row.on_hover_text_at_pointer("Claude is working in this session") } else { row }
}

/// One message as a card: role label and accent stripe, then its rendered Markdown.
/// Messages that are only tool calls show as a single dimmed line.
fn message(ui: &mut egui::Ui, cache: &mut CommonMarkCache, m: &session::Message, pal: &theme::Palette) {
    if m.role == Role::User && is_interrupt_marker(&m.text) {
        // Claude's record of a stop: alone, after the prompt it stopped, or (merged by the
        // parser) before the next prompt. Each part shows in its place.
        let (before, after) = split_interrupt_marker(&m.text);
        if !before.is_empty() {
            ui.push_id("before", |ui| message(ui, cache, &session::Message { role: Role::User, text: before }, pal));
        }
        ui.label(RichText::new("⏹ Interrupted").color(pal.note).monospace());
        ui.add_space(6.0);
        if !after.is_empty() {
            ui.push_id("after", |ui| message(ui, cache, &session::Message { role: Role::User, text: after }, pal));
        }
        return;
    }
    if m.role == Role::Assistant && m.text.lines().all(|l| l.starts_with("[tool:")) {
        if markdown::SHOW_TOOLS.load(std::sync::atomic::Ordering::Relaxed) {
            egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 0, top: 0, bottom: 6 }).show(ui, |ui| markdown::show(ui, cache, &m.text));
        }
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
        let ctx = ui.ctx().clone();
        self.poll_runner();
        self.poll_summary();
        self.poll_btw();
        self.poll_snapshots();
        self.poll_ai();
        self.poll_refine();
        self.poll_dictation();
        self.global_keys(&ctx);
        self.keep_zoom(&ctx);
        // First, so it spans the whole width above the side panels.
        egui::Panel::top("title-bar").exact_size(TITLE_BAR_HEIGHT).resizable(false).show(ui, |ui| self.title_bar(ui));
        egui::Panel::left("sessions").default_size(260.0).resizable(true).show(ui, |ui| self.sidebar(ui));
        egui::Panel::right("diffs").default_size(420.0).resizable(true).show(ui, |ui| self.right_panel(ui));
        let mut find_bar = None;
        egui::CentralPanel::default().show(ui, |ui| {
            if !self.file_tabs.is_empty() || self.settings_tab {
                egui::Panel::top("main-tabs").show_separator_line(true).resizable(false).show(ui, |ui| self.main_tabs(ui));
            }
            if self.settings_active && self.settings_tab {
                self.settings_page(ui);
                return;
            }
            self.update_find();
            find_bar = self.find.open.then(|| {
                egui::Panel::top("find-bar").show_separator_line(true).resizable(false).show(ui, |ui| self.find_bar(ui)).response.rect
            });
            let area_height = ui.available_height();
            egui::Panel::bottom("prompt")
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
                .show_separator_line(false)
                .resizable(false)
                .show(ui, |ui| self.prompt_bar(ui, area_height));
            match self.main_tab.filter(|&i| i < self.file_tabs.len()) {
                Some(i) => self.file_tab(ui, i),
                None => self.conversation(ui),
            }
            // A plain click anywhere in the main area goes to the input, like a terminal;
            // unless it went to another text field (refine's "Other…"), which keeps the focus.
            let area = ui.max_rect();
            let click = ui.input(|i| i.pointer.primary_clicked().then_some(i.pointer.interact_pos()).flatten());
            let on_other_field = |p| {
                let focused = ui.memory(|m| m.focused()).filter(|&f| f != Id::new("prompt-input"));
                focused.and_then(|f| ui.ctx().read_response(f)).is_some_and(|r| r.rect.contains(p))
            };
            let on_find_bar = |p| find_bar.is_some_and(|r: egui::Rect| r.contains(p));
            if click.is_some_and(|p| area.contains(p) && !on_other_field(p) && !on_find_bar(p)) {
                self.focus_prompt = true;
                ui.ctx().request_repaint();
            }
        });
        // The find bar hides on Esc, or on a press anywhere but on it. On the press, not the
        // click: its marks repaint the text and would hide a selection dragged from that press.
        let (esc, click) = ctx.input(|i| (i.key_pressed(Key::Escape), i.pointer.any_pressed().then_some(i.pointer.interact_pos()).flatten()));
        if self.find.open && (esc || click.is_some_and(|p| !find_bar.is_some_and(|r: egui::Rect| r.contains(p)))) {
            self.find.close();
            self.focus_prompt |= esc;
            ctx.request_repaint();
        }
        self.confirm_delete_modal(&ctx);
        self.model_prompt_modal(&ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if self.settings.restore_state {
            self.save_state();
        }
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_grows_to_its_limit_or_keeps_the_dragged_height() {
        // Automatic: one row, growing to max_lines rows.
        assert_eq!(input_heights(10.0, 10, None, false, 500.0), (10.0, 100.0));
        // Never taller than the area allows.
        assert_eq!(input_heights(10.0, 10, None, false, 60.0), (10.0, 60.0));
        // Dragged: exactly that height, clamped to one row .. limit.
        assert_eq!(input_heights(10.0, 10, Some(150.0), false, 500.0), (150.0, 150.0));
        assert_eq!(input_heights(10.0, 10, Some(2.0), false, 500.0), (10.0, 10.0));
        assert_eq!(input_heights(10.0, 10, Some(900.0), false, 500.0), (500.0, 500.0));
        // Collapsed: one line, whatever was dragged.
        assert_eq!(input_heights(10.0, 10, Some(150.0), true, 500.0), (10.0, 10.0));
    }

    /// Regression: after Ctrl+C the partial reply showed twice (sessman's copy and Claude's).
    #[test]
    fn a_saved_stop_hides_sessmans_own_copy() {
        let m = |role, text: &str| session::Message { role, text: text.into() };
        let before = vec![m(Role::User, "write a story"), m(Role::Assistant, "Once upon")];
        assert!(!saved_interrupt(&before));
        let mut after = vec![m(Role::User, "write a story"), m(Role::Assistant, "Once upon")];
        after.push(m(Role::User, "[Request interrupted by user]"));
        assert!(saved_interrupt(&after));
        // A new prompt after the stop: the old marker no longer counts.
        after.push(m(Role::User, "try again"));
        assert!(!saved_interrupt(&after));
        assert!(is_interrupt_marker("[Request interrupted by user for tool use]"));
        // Merged into the prompt it stopped, as the session file can hold it.
        assert!(saved_interrupt(&[m(Role::User, "write it [Request interrupted by user]")]));
        let split = |t| split_interrupt_marker(t);
        assert_eq!(split("write it [Request interrupted by user]"), ("write it".into(), String::new()));
        assert_eq!(split("[Request interrupted by user for tool use]"), (String::new(), String::new()));
        // Regression: the next prompt merged after the marker must show after it, not before.
        assert_eq!(split("[Request interrupted by user]\n\nReply five"), (String::new(), "Reply five".into()));
        // Regression: the pinned "You" showed a lone marker as if it were a prompt.
        assert_eq!(pinned_text(&m(Role::User, "[Request interrupted by user]")), None);
        assert_eq!(pinned_text(&m(Role::User, "[Request interrupted by user]\nReply five")).as_deref(), Some("Reply five"));
        assert_eq!(pinned_text(&m(Role::User, "write it")).as_deref(), Some("write it"));
        assert_eq!(pinned_text(&m(Role::Assistant, "hi")), None);
    }

    /// Regression (item 5): a new session shows in the list as soon as its first prompt is
    /// sent, not only once the indexer finds its file.
    #[test]
    fn new_sessions_are_listed_while_their_first_prompt_runs() {
        let run = |new: bool, prompt: &str| Run {
            rx: std::sync::mpsc::channel().1,
            live: String::new(),
            tool: None,
            new,
            cwd: Some(PathBuf::from("/tmp/play")),
            prompt: prompt.into(),
            stop: runner::Stop::default(),
            poked: false,
        };
        let mut runs = HashMap::new();
        runs.insert("n1".to_owned(), run(true, "build it\nwith details"));
        runs.insert("old".to_owned(), run(false, "continue"));
        runs.insert("n2".to_owned(), run(true, "listed already"));
        let listed = vec![SessionMeta { id: "n2".into(), path: PathBuf::new(), project: String::new(), title: String::new(), modified: UNIX_EPOCH }];
        let pending = pending_new(&runs, &listed, None);
        assert_eq!(pending, vec![("n1".to_owned(), "build it".to_owned(), "/tmp/play".to_owned())]);
        // In a workspace tab, only the sessions created there.
        assert!(pending_new(&runs, &listed, Some(&["other".to_owned()])).is_empty());
        assert_eq!(pending_new(&runs, &listed, Some(&["n1".to_owned()])).len(), 1);
    }

    #[test]
    fn closing_a_tab_keeps_the_right_one_active() {
        assert_eq!(after_close(Some(2), 2), None);
        assert_eq!(after_close(Some(3), 1), Some(2));
        assert_eq!(after_close(Some(0), 1), Some(0));
        assert_eq!(after_close(None, 0), None);
    }
}
