use crate::control::{self, Command};
use crate::diffview::{self, CardEvent};
use crate::dictation;
use crate::find::{self, Find};
use crate::goal;
use crate::history::History;
use crate::images;
use crate::markdown;
use crate::names::{self, Names};
use crate::orchestration;
use crate::overview::{self, Overviews};
use crate::queue::Queue;
use crate::refine;
use crate::runner::{self, RunEvent};
use crate::search::{self, Hit, Source};
use crate::session::{self, Role, Session, SessionMeta};
use crate::settings::{self, Settings};
use crate::shared::{self, Shared};
use crate::state::{self, OpenFile, State, View, Views};
use crate::status;
use crate::theme;
use crate::verdict::{self, Verdict, Verdicts};
use crate::workspaces::{self, Workspaces};
use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{self, Align2, Color32, Id, Key, LayerId, Modifiers, Order, RichText, Sense, Stroke, StrokeKind};
use egui_commonmark::CommonMarkCache;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Height of the bar at the top of the window, where the macOS window buttons sit.
pub const TITLE_BAR_HEIGHT: f32 = 28.0;

/// A session the bar draws: a listed one (with its search note), or a new one still starting
/// (id, its first prompt's first line).
enum BarItem {
    Listed(usize, Option<String>),
    Starting(String, String),
}

/// Sidebar interactions, collected while drawing and applied afterwards.
enum Action {
    Select(usize),
    Tab(Option<usize>),
    MoveTo(Vec<String>, usize),
    /// Take these sessions out of the open workspace, then open the one given (if any).
    Remove(Vec<String>, Option<String>),
    /// The sessions picked for moving or deleting together (Shift/Cmd-click), replacing the
    /// picked ones; the session clicked becomes where a Shift-click range starts.
    Pick(HashSet<String>, String),
    /// Ask to move these sessions to the Trash.
    Trash(Vec<String>),
    DeleteWorkspace(usize),
    RenameWorkspace(usize, String),
    ColorWorkspace(usize, [u8; 3]),
    Rename(String, String),
    CancelRename,
    /// Back to a new session whose first prompt is running (it isn't on disk yet).
    OpenPending(String),
    /// The open workspace's orchestration page.
    Orchestrate,
    /// The open workspace's bar in this order, dragged into it.
    ArrangeBar(Vec<String>),
    /// The open workspace's bar newest first again.
    BarNewestFirst,
    /// A workspace tab dragged to another place among the tabs.
    MoveTab(usize, usize),
}

/// What a dragged workspace tab carries: its index.
struct TabDrag(usize);

/// Sessions one action took out of workspaces, and the session open before it.
struct Removal {
    taken: Vec<workspaces::Taken>,
    open: Option<String>,
}

/// How many removals Cmd+Z can go back through.
const UNDO_LIMIT: usize = 50;

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
    /// Workspace whose tab is being renamed in place, as in `Renaming`.
    workspace_rename: Option<(usize, String, bool)>,
    /// Workspace waiting for "Delete?" to be confirmed.
    confirm_delete: Option<usize>,
    /// Sessions picked with Shift/Cmd-click or a drawn box, to move or delete together.
    picked: HashSet<String>,
    /// Session a Shift-click range starts from: the one last clicked.
    pick_anchor: Option<String>,
    /// A box being drawn over the session list: its start, from the list's top-left (so it
    /// scrolls with the list), and what was picked before (Shift adds to it).
    lasso: Option<(egui::Vec2, HashSet<String>)>,
    /// Sessions waiting for "Move to Trash?" to be confirmed.
    confirm_trash: Vec<String>,
    /// Message waiting for "Delete from the context?": session id, index, text (as in `session::delete_message`).
    confirm_delete_message: Option<(String, usize, String)>,
    /// Index into the open session's messages of the "You" prompt pinned above the conversation.
    pinned: Option<usize>,
    /// Whether a "You" prompt was on screen in the conversation last frame; the pinned one hides then.
    inline_you: bool,
    /// Index into the open session's messages of the one picked in the conversation (a click
    /// on it, or the arrows).
    picked_message: Option<usize>,
    /// Whether the conversation has the keyboard (it was clicked last): Up/Down then move the
    /// pick a message at a time, bringing its start to the top.
    conversation_keys: bool,
    /// Whether the session bar has the keyboard (a session in it was clicked last): Up/Down
    /// then open the session above or below, Delete/Backspace take the highlighted ones out of
    /// the open workspace.
    bar_keys: bool,
    /// Sessions taken out of workspaces, latest last, for Cmd+Z to put back.
    removed: Vec<Removal>,
    /// Set when the arrows moved the pick: the conversation scrolls to it on the next frame.
    scroll_to_pick: bool,
    /// The message at the top of the conversation last frame, where the arrows start without a pick.
    top_message: Option<usize>,
    /// Where the conversation showed this frame (screen), if it did; clicks there go to it.
    conversation_rect: Option<egui::Rect>,
    names: Shared<Names>,
    renaming: Option<Renaming>,
    /// New sessions whose first run ended before the indexer listed them: (id, first prompt,
    /// folder). They stay in the bar as starting until it does.
    unlisted: Vec<(String, String, Option<PathBuf>)>,
    /// Folder of a new session that has no prompt yet (nothing exists on disk until then).
    new_session: Option<PathBuf>,
    /// What the new session will start with, gathered once per folder.
    new_info: Option<(PathBuf, crate::newinfo::NewInfo)>,
    /// Id given to the new session's first prompt, to select it once it exists.
    new_id: Option<String>,
    /// Set to move keyboard focus to the prompt input on the next frame.
    focus_prompt: bool,
    right_tab: RightTab,
    /// AI-written one-line goals, keyed by session id.
    goals: Shared<Names>,
    /// Session id whose goal is being summarized, and the pending result.
    summarizing: Option<(String, Receiver<Result<String, String>>)>,
    /// New sessions whose name the helper model is writing after their first answer.
    naming: Vec<(String, Receiver<Result<String, String>>)>,
    /// Side questions per session id, oldest first; kept until closed with their ×.
    btw: HashMap<String, Vec<Btw>>,
    /// Text in the Info tab's "ask about this goal" field.
    goal_question: String,
    /// Heights measured last frame: the Info sections, and the question field. The questions
    /// area grows upwards until it meets the sections.
    info_height: f32,
    goal_field_height: f32,
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
    settings: Shared<Settings>,
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
    updater: crate::update::Updater,
    /// Recent microphone levels (0..1, oldest first) for the meter shown while recording.
    mic_levels: std::collections::VecDeque<f32>,
    /// Background indexer: fresh session lists + content index, and a channel to poke it.
    snapshots: Receiver<search::Snapshot>,
    poke_index: std::sync::mpsc::Sender<()>,
    /// A shared config file was written (`shared::watch`), maybe by another scope.
    shared_changes: Receiver<()>,
    /// The zoom the window had last frame (see `keep_zoom`).
    zoom_seen: f32,
    /// Prompt runs this scope did not start (see `runner::watch_runs`): session id, pid.
    foreign_runs: Receiver<Vec<(String, u32)>>,
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
    /// Workspace summaries for the overview in All sessions, those being written (by
    /// workspace name), and the last failure of each (not retried until asked).
    overviews: Shared<Overviews>,
    overview_jobs: HashMap<String, Receiver<Result<overview::Summary, String>>>,
    overview_errors: HashMap<String, String>,
    /// Session id → its newest change (Unix seconds) when it was last open, and when scope
    /// started keeping this (sessions it has never had open count as seen then). A session
    /// changed since is marked finished in the overview.
    seen: HashMap<String, u64>,
    /// Session id → the model and effort picked for it on the new session page.
    choices: HashMap<String, runner::Choice>,
    /// What the new session page has picked so far.
    new_choice: runner::Choice,
    /// The model a session's runs get when it picked none, for the folder, Settings model and
    /// extra arguments it was read with, and when (read again after a few seconds).
    default_model: Option<((PathBuf, String, String), std::time::Instant, Option<crate::newinfo::Sourced>)>,
    seen_from: u64,
    /// Whether each session's work looks finished (verdict.rs), the checks under way, the
    /// checks that failed (id → the change they read; not retried until there is a newer one),
    /// and when workspace sessions were last looked over for checks to start.
    verdicts: Shared<Verdicts>,
    judging: Vec<(String, Receiver<Result<Verdict, String>>)>,
    judge_failed: HashMap<String, u64>,
    last_sweep: Option<std::time::Instant>,
    /// The main area shows the open workspace's orchestration page (with no session open).
    orchestrating: bool,
    /// Each workspace's orchestration report and questions (by name), and those in flight.
    orchestrations: Shared<orchestration::Store>,
    orch: orchestration::Jobs,
    /// Commands from `scope list|rename|move|new|ask` (control.rs).
    control: Receiver<control::Incoming>,
    /// Account limits from the usage endpoint (`status::watch_usage`): the latest answer and
    /// when it came, the last failure, and whether Claude is working (asks more often then).
    usage_rx: Receiver<Result<Vec<status::Window>, String>>,
    usage: Option<(Vec<status::Window>, SystemTime)>,
    usage_error: Option<String>,
    usage_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
    /// Text Claude has written in this run (capped), to name a new session after it.
    answer: String,
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
/// A button's tooltip: what it does, then its shortcut, shown whether or not it is enabled
/// (a disabled one adds why, so the shortcut is still there to learn).
fn tip(resp: egui::Response, what: &str, why_not: Option<&str>, keys: &str) -> egui::Response {
    let body = |ui: &mut egui::Ui, extra: Option<&str>| {
        ui.label(what);
        if let Some(e) = extra {
            ui.label(RichText::new(e).italics());
        }
        ui.label(RichText::new(keys).monospace().weak());
    };
    resp.on_hover_ui(|ui| body(ui, None)).on_disabled_hover_ui(|ui| body(ui, why_not))
}

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
        // Thumbnails of the images in prompts (`file://` URIs).
        egui_extras::install_image_loaders(ctx);
        let (snapshots, poke_index) = search::spawn_indexer(ctx.clone());
        let settings: Shared<Settings> = Shared::load(settings::path());
        let check_for_updates = settings.check_for_updates;
        let shared_changes = shared::watch(
            ctx.clone(),
            vec![Workspaces::path(), Names::path(), Names::goals_path(), settings::path(), Overviews::path(), orchestration::Store::path(), Verdicts::path()],
        );
        let usage_busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
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
            workspace_rename: None,
            confirm_delete: None,
            picked: HashSet::new(),
            pick_anchor: None,
            lasso: None,
            confirm_trash: Vec::new(),
            confirm_delete_message: None,
            pinned: None,
            picked_message: None,
            conversation_keys: false,
            bar_keys: false,
            removed: Vec::new(),
            scroll_to_pick: false,
            top_message: None,
            conversation_rect: None,
            inline_you: false,
            names: Shared::load(Names::path()),
            renaming: None,
            unlisted: Vec::new(),
            new_session: None,
            new_info: None,
            new_id: None,
            focus_prompt: false,
            right_tab: RightTab::Changes,
            goals: Shared::load(Names::goals_path()),
            summarizing: None,
            naming: Vec::new(),
            btw: HashMap::new(),
            goal_question: String::new(),
            info_height: 0.0,
            goal_field_height: 0.0,
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
            updater: crate::update::Updater::new(check_for_updates),
            mic_levels: std::collections::VecDeque::new(),
            snapshots,
            poke_index,
            shared_changes,
            zoom_seen: ctx.zoom_factor(),
            foreign_runs: runner::watch_runs(ctx.clone()),
            usage_rx: status::watch_usage(ctx.clone(), usage_busy.clone()),
            usage: None,
            usage_error: None,
            usage_busy,
            index: search::Index::new(),
            generation: 0,
            search: String::new(),
            hits: None,
            ai: None,
            saved_state: String::new(),
            find: Find::default(),
            overviews: Shared::load(Overviews::path()),
            overview_jobs: HashMap::new(),
            overview_errors: HashMap::new(),
            seen: HashMap::new(),
            choices: HashMap::new(),
            new_choice: runner::Choice::default(),
            default_model: None,
            seen_from: 0,
            verdicts: Shared::load(Verdicts::path()),
            judging: Vec::new(),
            judge_failed: HashMap::new(),
            last_sweep: None,
            orchestrating: false,
            orchestrations: Shared::load(orchestration::Store::path()),
            orch: orchestration::Jobs::default(),
            control: control::watch(ctx.clone()),
        };
        app.apply_settings();
        // Persist colors assigned on load to workspaces saved before colors existed.
        if !app.workspaces.list.is_empty() {
            app.save_workspaces();
        }
        let mut st = State::load(&state::path());
        app.seen = std::mem::take(&mut st.seen);
        app.choices = std::mem::take(&mut st.choices);
        app.seen_from = match st.seen_from {
            0 => overview::secs(SystemTime::now()),
            t => t,
        };
        if app.settings.restore_state {
            app.restore(st);
        }
        app
    }

    /// Puts back what was open when scope last closed.
    fn restore(&mut self, mut st: State) {
        st.prune(|id| self.sessions.iter().any(|s| s.id == id));
        self.tab = st.tab.as_ref().and_then(|name| self.workspaces.list.iter().position(|w| &w.name == name));
        self.views = Views(st.views);
        self.queues = st.queues;
        self.right_tab = if st.info_tab { RightTab::Info } else { RightTab::Changes };
        self.search = st.search;
        self.prompt_height = st.prompt_height;
        self.prompt_collapsed = st.prompt_collapsed;
        match st.open.and_then(|id| self.sessions.iter().position(|s| s.id == id)) {
            Some(i) => self.select(i),
            // A workspace that was on its orchestration page (or empty) opens on it again.
            None if self.tab.is_some() => self.open_tab(),
            None => {}
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
            seen: self.seen.iter().filter(|(id, _)| self.sessions.iter().any(|s| s.id == **id)).map(|(k, v)| (k.clone(), *v)).collect(),
            seen_from: self.seen_from,
            choices: self.choices.clone(),
        }
    }

    /// The open session counts as seen as it is now, once its run is over.
    fn mark_seen(&mut self) {
        if self.new_session.is_some() {
            return;
        }
        let Some(s) = self.selected.and_then(|i| self.sessions.get(i)) else { return };
        let modified = overview::secs(s.modified);
        if !self.runs.contains_key(&s.id) && self.seen.get(&s.id).is_none_or(|&t| t < modified) {
            self.seen.insert(s.id.clone(), modified);
        }
    }

    /// Its last run ended after it was last open (or, never open here, after scope started
    /// keeping track), and it isn't working: it waits to be looked at.
    fn finished_unseen(&self, i: usize) -> bool {
        let s = &self.sessions[i];
        let seen = self.seen.get(&s.id).copied().unwrap_or(self.seen_from);
        !self.runs.contains_key(&s.id) && overview::secs(s.modified) > seen && self.selected != Some(i)
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
        self.updater.auto_check = self.settings.check_for_updates;
        runner::configure(runner::Claude::from_settings(&self.settings));
        markdown::SHOW_TOOLS.store(self.settings.show_tool_lines, std::sync::atomic::Ordering::Relaxed);
    }

    /// Takes workspace changes another scope saved, so a change made here doesn't overwrite
    /// them. `w`, a workspace index from before, comes back for the same workspace (None if
    /// it is gone); the open tab follows its workspace the same way.
    fn sync_workspaces(&mut self, w: Option<usize>) -> Option<usize> {
        let name = |ws: &Workspaces, i: Option<usize>| i.and_then(|i| ws.list.get(i)).map(|w| w.name.clone());
        let (tab, w_name) = (name(&self.workspaces, self.tab), name(&self.workspaces, w));
        if !self.workspaces.refresh(&Workspaces::path()) {
            return w;
        }
        let list = &self.workspaces.list;
        let find = |n: Option<String>| n.and_then(|n| list.iter().position(|x| x.name == n));
        self.tab = find(tab);
        find(w_name)
    }

    /// Takes what other scopes saved to the config files they share with this one: workspaces
    /// (members, order, names, colors), session names, goals, settings, overviews,
    /// orchestrations and session tags, so every scope on one config folder shows the same.
    fn poll_shared(&mut self) {
        if self.shared_changes.try_iter().count() == 0 {
            return;
        }
        self.sync_workspaces(None);
        // A session another scope just put in a workspace (one it started, say) is listed as
        // soon as its file is there, not at the next pass of the indexer.
        let listed: HashSet<&str> = self.sessions.iter().map(|s| s.id.as_str()).collect();
        if self.workspaces.list.iter().flat_map(|w| &w.sessions).any(|id| !listed.contains(id.as_str())) {
            let _ = self.poke_index.send(());
        }
        self.names.refresh();
        self.goals.refresh();
        self.overviews.refresh();
        self.orchestrations.refresh();
        self.verdicts.refresh();
        if self.settings.refresh() {
            self.apply_settings();
        }
    }

    fn save_workspaces(&mut self) {
        if let Err(e) = self.workspaces.save(&Workspaces::path()) {
            self.error = Some(format!("could not save scopes: {e}"));
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Select(i) => {
                self.picked.clear();
                self.pick_anchor = Some(self.sessions[i].id.clone());
                self.select(i);
            }
            Action::Tab(t) => self.switch_tab(t),
            Action::ArrangeBar(order) => {
                if let Some(w) = self.sync_workspaces(self.tab) {
                    self.workspaces.arrange_bar(w, &order);
                    self.save_workspaces();
                }
            }
            Action::BarNewestFirst => {
                if let Some(w) = self.sync_workspaces(self.tab) {
                    self.workspaces.unarrange_bar(w);
                    self.save_workspaces();
                }
            }
            Action::MoveTab(from, to) => {
                let names: Vec<String> = self.workspaces.list.iter().map(|w| w.name.clone()).collect();
                let Some(name) = names.get(from) else { return };
                // Indices from before another scope's changes, found again by name.
                let at = |n: Option<&String>, list: &[workspaces::Workspace]| n.and_then(|n| list.iter().position(|w| w.name == *n));
                let (to_name, tab_name) = (names.get(to).cloned(), self.tab.and_then(|t| names.get(t)).cloned());
                self.sync_workspaces(None);
                let list = &self.workspaces.list;
                let Some(from) = at(Some(name), list) else { return };
                let to = at(to_name.as_ref(), list).unwrap_or(list.len());
                self.workspaces.reorder(from, to);
                self.tab = at(tab_name.as_ref(), &self.workspaces.list);
                self.workspace_rename = None;
                self.confirm_delete = None;
                self.save_workspaces();
            }
            Action::MoveTo(ids, w) => {
                if let Some(w) = self.sync_workspaces(Some(w)) {
                    for id in &ids {
                        self.workspaces.move_to(id, w);
                    }
                    self.save_workspaces();
                }
            }
            Action::Remove(ids, then) => {
                self.sync_workspaces(None);
                let taken: Vec<workspaces::Taken> = ids.iter().filter_map(|id| self.workspaces.take(id)).collect();
                self.save_workspaces();
                if !taken.is_empty() {
                    let open = self.selected.map(|i| self.sessions[i].id.clone());
                    self.removed.push(Removal { taken, open });
                    let extra = self.removed.len().saturating_sub(UNDO_LIMIT);
                    self.removed.drain(..extra);
                }
                self.picked.retain(|id| !ids.contains(id));
                let open_removed = self.selected.is_some_and(|i| ids.contains(&self.sessions[i].id));
                match then.and_then(|id| self.sessions.iter().position(|s| s.id == id)) {
                    Some(i) => {
                        self.pick_anchor = Some(self.sessions[i].id.clone());
                        self.select(i);
                    }
                    // The open session left the workspace and none is left to open in its place.
                    None if open_removed && self.tab.is_some() => self.open_tab(),
                    None => {}
                }
            }
            Action::Pick(ids, anchor) => {
                self.picked = ids;
                self.pick_anchor = Some(anchor);
            }
            Action::Trash(ids) => self.confirm_trash = ids,
            Action::Rename(id, name) => {
                self.renaming = None;
                if let Err(e) = self.names.change(|n| n.set(&id, &name)) {
                    self.error = Some(format!("could not save names: {e}"));
                }
            }
            Action::CancelRename => self.renaming = None,
            Action::DeleteWorkspace(w) if self.settings.confirm_delete_workspace => self.confirm_delete = Some(w),
            Action::DeleteWorkspace(w) => self.delete_workspace(w),
            Action::RenameWorkspace(w, name) => self.rename_workspace(w, &name),
            Action::ColorWorkspace(w, c) => {
                if let Some(w) = self.sync_workspaces(Some(w)) {
                    self.workspaces.set_color(w, c);
                    self.save_workspaces();
                }
            }
            Action::OpenPending(id) => self.open_pending(&id),
            Action::Orchestrate => self.show_orchestration(),
        }
    }

    /// Cmd+Z: puts the sessions taken out last back in their workspace, and opens again the
    /// session that was open then (when this tab lists it), the bar keeping the keyboard.
    fn undo_remove(&mut self) {
        let Some(r) = self.removed.pop() else { return };
        self.sync_workspaces(None);
        let back: Vec<String> = r.taken.iter().rev().filter(|t| self.workspaces.put_back(t)).map(|t| t.session.clone()).collect();
        if back.is_empty() {
            return;
        }
        self.save_workspaces();
        let visible = self.visible();
        let Some(i) = r.open.and_then(|id| visible.into_iter().find(|&i| self.sessions[i].id == id)) else { return };
        let keys = self.bar_keys;
        self.select(i);
        self.pick_anchor = Some(self.sessions[i].id.clone());
        self.picked = if back.len() > 1 { back.into_iter().collect() } else { HashSet::new() };
        if keys {
            self.focus_prompt = false;
        }
    }

    /// Renames a workspace, its overview summary and orchestration following it.
    fn rename_workspace(&mut self, w: usize, name: &str) {
        self.workspace_rename = None;
        // Escape, or a name cleared: nothing changes.
        if name.trim().is_empty() {
            return;
        }
        let Some(w) = self.sync_workspaces(Some(w)) else { return };
        let old = self.workspaces.list[w].name.clone();
        if let Err(e) = self.workspaces.rename(w, name) {
            self.error = Some(format!("could not rename scope: {e}"));
            return;
        }
        self.save_workspaces();
        let new = self.workspaces.list[w].name.clone();
        if new == old {
            return;
        }
        if let Err(e) = self.overviews.change(|o| o.rename(&old, &new)) {
            self.error = Some(format!("could not save overviews: {e}"));
        }
        self.orchestrations.refresh();
        self.orchestrations.rename(&old, &new);
        self.save_orchestrations();
    }

    fn delete_workspace(&mut self, w: usize) {
        let Some(w) = self.sync_workspaces(Some(w)) else { return };
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

    /// Moves sessions' files to the Trash and drops them from the list and the workspaces.
    /// Sessions Claude is working in are skipped.
    fn trash_sessions(&mut self, ids: &[String]) {
        let mut trashed = HashSet::new();
        let mut failed = Vec::new();
        for id in ids {
            let Some(i) = self.sessions.iter().position(|s| &s.id == id) else { continue };
            if self.runs.contains_key(id) {
                failed.push(format!("\"{}\": Claude is working in it", self.title(i)));
                continue;
            }
            let path = &self.sessions[i].path;
            if let Err(e) = runner::trash(path) {
                failed.push(format!("\"{}\": {e}", self.title(i)));
                continue;
            }
            // Claude keeps a session's subagent and tool output in a folder named like its file.
            let dir = path.with_extension("");
            if dir.is_dir() {
                let _ = runner::trash(&dir);
            }
            trashed.insert(id.clone());
        }
        if !failed.is_empty() {
            self.error = Some(format!("could not move to the Trash: {}", failed.join("; ")));
        }
        if trashed.is_empty() {
            return;
        }
        let open = self.selected.map(|i| self.sessions[i].id.clone());
        self.sessions.retain(|s| !trashed.contains(&s.id));
        self.selected = open.as_ref().and_then(|id| self.sessions.iter().position(|s| &s.id == id));
        self.generation += 1;
        self.picked.retain(|id| !trashed.contains(id));
        self.sync_workspaces(None);
        for id in &trashed {
            self.workspaces.remove(id);
        }
        self.save_workspaces();
        if open.is_some_and(|id| trashed.contains(&id)) {
            self.open_tab();
        }
        let _ = self.poke_index.send(());
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
        self.new_session = None;
        self.orchestrating = false;
        self.focus_prompt = true;
        self.sync_workspaces(None);
        if self.workspaces.set_last(self.tab, &id) {
            self.save_workspaces();
        }
        self.reload();
    }

    /// A workspace tab (or All) was clicked: it shows the session last open there.
    fn switch_tab(&mut self, t: Option<usize>) {
        if self.tab == t {
            if t.is_none() {
                self.show_overview();
            }
            return;
        }
        self.tab = t;
        self.picked.clear();
        // A search belongs to the scope it was typed in.
        self.search.clear();
        self.open_tab();
    }

    /// Opens what was last open in the current workspace: a session (else its newest), or its
    /// orchestration page, which an empty workspace also opens on. All sessions opens on the
    /// overview.
    fn open_tab(&mut self) {
        let Some(t) = self.tab else { return self.show_overview() };
        if self.workspaces.last(Some(t)) == Some(orchestration::LAST_KEY) {
            return self.show_orchestration();
        }
        let visible = self.visible();
        let ids: Vec<&str> = visible.iter().map(|&i| self.sessions[i].id.as_str()).collect();
        match workspaces::pick_session(self.workspaces.last(Some(t)), &ids) {
            Some(k) => self.select(visible[k]),
            None => self.show_orchestration(),
        }
    }

    /// Nothing open in the main area: an empty workspace, or the overview in All sessions.
    fn close_session(&mut self) {
        self.switch_view(None);
        self.selected = None;
        self.open = None;
        self.new_session = None;
        self.history = History::default();
        self.pinned = None;
        self.picked_message = None;
        self.top_message = None;
        self.inline_you = false;
    }

    fn show_overview(&mut self) {
        self.close_session();
        self.orchestrating = false;
        self.main_tab = None;
        self.settings_active = false;
    }

    /// The open workspace's orchestration page in the main area; the tab reopens on it.
    fn show_orchestration(&mut self) {
        let Some(t) = self.tab else { return };
        self.close_session();
        self.orchestrating = true;
        self.main_tab = None;
        self.settings_active = false;
        self.sync_workspaces(None);
        if self.workspaces.set_last(Some(t), orchestration::LAST_KEY) {
            self.save_workspaces();
        }
    }

    fn showing_orchestration(&self) -> bool {
        self.orchestrating && self.tab.is_some() && self.open.is_none() && !(self.settings_active && self.settings_tab)
    }

    /// Workspace `w`'s sessions as the orchestrator reads them, newest first: new ones still
    /// starting (nothing on disk to read yet), then the listed ones.
    fn orchestration_members(&self, w: usize) -> Vec<orchestration::Member> {
        let now = overview::secs(SystemTime::now());
        let starting = pending_new(&self.runs, &self.unlisted, &self.sessions, Some(&self.workspaces.list[w].sessions));
        let starting = starting.into_iter().map(|(id, ..)| orchestration::Member {
            title: self.name_of(&id),
            goal: None,
            path: PathBuf::new(),
            modified: now,
            running: self.runs.contains_key(&id),
            queued: self.queues.get(&id).map_or(0, |q| q.items.len()),
            id,
        });
        starting.chain(self.workspace_sessions(w).into_iter()
            .map(|i| {
                let s = &self.sessions[i];
                orchestration::Member {
                    id: s.id.clone(),
                    title: self.title(i).to_owned(),
                    goal: self.goals.get(&s.id).map(str::to_owned),
                    path: s.path.clone(),
                    modified: overview::secs(s.modified),
                    running: self.runs.contains_key(&s.id),
                    queued: self.queues.get(&s.id).map_or(0, |q| q.items.len()),
                }
            }))
            .collect()
    }

    /// Updates workspace `w`'s report with what is new, or (`fresh`, or no report yet) writes
    /// it from scratch. Nothing if one is already being written.
    fn update_orchestration(&mut self, w: usize, fresh: bool) {
        let name = self.workspaces.list[w].name.clone();
        let members = self.orchestration_members(w);
        if members.is_empty() || self.orch.updating.contains_key(&name) {
            return;
        }
        let previous = if fresh { None } else { self.orchestrations.get(&name).cloned() };
        let harness = Settings::harness(&self.settings.orchestration_harness, orchestration::DEFAULT_HARNESS).to_owned();
        let rx = orchestration::update(self.ctx.clone(), harness, self.settings.orchestration_model.clone(), members, previous);
        self.orch.errors.remove(&name);
        self.orch.updating.insert(name, rx);
    }

    /// Saves the orchestrations; a change to them is made after `self.orchestrations.refresh()`
    /// (or within `change`), so it lands on top of what other scopes saved.
    fn save_orchestrations(&mut self) {
        if let Err(e) = self.orchestrations.save() {
            self.error = Some(format!("could not save the orchestration: {e}"));
        }
    }

    /// Takes finished orchestration updates and answers, for every workspace.
    fn poll_orchestration(&mut self) {
        let now = overview::secs(SystemTime::now());
        let mut changed = false;
        let mut done = Vec::new();
        for (name, rx) in &self.orch.updating {
            match rx.try_recv() {
                Ok(result) => done.push((name.clone(), result)),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => done.push((name.clone(), Err("stopped unexpectedly".into()))),
            }
        }
        for (name, result) in done {
            self.orch.updating.remove(&name);
            match result {
                Ok(u) => {
                    self.orchestrations.refresh();
                    self.orchestrations.entry(&name).apply(u, now);
                    changed = true;
                }
                Err(e) => {
                    self.orch.errors.insert(name, e);
                }
            }
        }
        let mut answered = Vec::new();
        for (name, (_, rx)) in &self.orch.asking {
            match rx.try_recv() {
                Ok(result) => answered.push((name.clone(), result)),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => answered.push((name.clone(), Err("stopped unexpectedly".into()))),
            }
        }
        for (name, result) in answered {
            let Some((question, _)) = self.orch.asking.remove(&name) else { continue };
            let asked_by = self.orch.asked_by.remove(&name);
            match result {
                Ok(answer) => {
                    // Sessions named as they were when it answered, before it renamed any.
                    let plain = asked_by.is_some().then(|| self.plain_refs(&answer.text));
                    let done: Vec<String> = answer
                        .actions
                        .into_iter()
                        .map(|c| {
                            // New sessions go in the orchestrator's own workspace.
                            let c = match c {
                                Command::New { prompt, folder, name: n, .. } => Command::New { prompt, workspace: Some(name.clone()), folder, name: n },
                                c => c,
                            };
                            let request = control::Request { command: c, caller: None, cwd: None };
                            match self.execute(request, None) {
                                Some(Ok(d)) => d,
                                Some(Err(e)) => format!("Could not: {e}"),
                                None => String::new(),
                            }
                        })
                        .filter(|d| !d.is_empty())
                        .collect();
                    if let (Some(path), Some(mut text)) = (asked_by, plain) {
                        for d in &done {
                            text.push_str(&format!("\n- {d}"));
                        }
                        control::write_reply(&path, &Ok(text));
                    }
                    self.orchestrations.refresh();
                    self.orchestrations.entry(&name).answered(question, answer.text, done, now);
                    changed = true;
                }
                Err(e) => {
                    if let Some(path) = asked_by {
                        control::write_reply(&path, &Err(format!("the orchestrator could not answer: {e}")));
                    }
                    // The question goes back to the field to try again.
                    if self.orch.question.trim().is_empty() {
                        self.orch.question = question;
                    }
                    self.orch.ask_errors.insert(name, e);
                }
            }
        }
        if changed {
            self.save_orchestrations();
        }
    }

    /// The open workspace's orchestration page. The report updates on its own when it is
    /// missing, or when there is news and the last update is old enough (Settings).
    fn orchestration(&mut self, ui: &mut egui::Ui) {
        let Some(w) = self.tab.filter(|&w| w < self.workspaces.list.len()) else { return };
        let name = self.workspaces.list[w].name.clone();
        let members = self.orchestration_members(w);
        let now = overview::secs(SystemTime::now());
        if !self.orch.updating.contains_key(&name)
            && !self.orch.errors.contains_key(&name)
            && orchestration::due(self.orchestrations.get(&name), &members, self.settings.orchestration_every, now)
        {
            self.update_orchestration(w, false);
        }
        // Checked again every minute while the page is open, for news to report.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(60));
        // Cards in the bar's order; the model still reads the sessions newest first.
        let mut shown: Vec<&orchestration::Member> = members.iter().collect();
        let ws = &self.workspaces.list[w];
        if ws.ordered {
            shown.sort_by_key(|m| ws.sessions.iter().position(|id| *id == m.id));
        }
        let live: Vec<orchestration::Live> = shown
            .into_iter()
            .map(|m| {
                let checklist = self.goals.get(&m.id).map(goal::parse).map(|g| (g.items.iter().filter(|(d, _)| *d).count(), g.items.len()));
                orchestration::Live {
                    id: &m.id,
                    title: &m.title,
                    running: m.running,
                    tool: self.runs.get(&m.id).and_then(|r| r.tool.as_deref()),
                    queued: m.queued,
                    modified: m.modified,
                    checklist,
                    verdict: (!m.running).then(|| self.verdicts.current(&m.id, m.modified)).flatten(),
                }
            })
            .collect();
        // Report / Processes tabs. Processes are sampled while the page is open, so the tab
        // can say how many there are.
        let color = self.workspaces.list[w].color.unwrap_or([128, 128, 128]);
        let accent = egui::Color32::from_rgb(color[0], color[1], color[2]);
        self.orch.processes.poll(ui.ctx(), &name, members.iter().map(|m| (m.id.clone(), m.path.clone())).collect());
        let sample = self.orch.processes.of(&name);
        let count = sample.map(|g| g.iter().map(|g| g.rows.len()).sum::<usize>());
        egui::Panel::top("orchestration-tabs").show_separator_line(true).resizable(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(!self.orch.show_processes, RichText::new("Report").strong()).clicked() {
                    self.orch.show_processes = false;
                }
                let label = match count {
                    Some(n) if n > 0 => format!("Processes · {n}"),
                    _ => "Processes".to_owned(),
                };
                let tab = ui.selectable_label(self.orch.show_processes, RichText::new(label).strong());
                if tab.on_hover_text("What this scope's sessions are running now: CPU, RAM, how long, time left").clicked() {
                    self.orch.show_processes = true;
                }
            });
        });
        if self.orch.show_processes {
            let names: HashMap<&str, &str> = live.iter().map(|s| (s.id, s.title)).collect();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(crate::workspaces::label(&name));
                ui.label(RichText::new("processes").weak());
            });
            match crate::processes::ui(ui, sample, &names, &self.orch.processes.stopping, accent) {
                Some(crate::processes::Event::Open(id)) => match self.sessions.iter().position(|s| s.id == id) {
                    Some(i) => self.select(i),
                    None => self.open_pending(&id),
                },
                Some(crate::processes::Event::StopClaude(id, pid)) => {
                    // Scope's own runs (and the ones it adopted) stop like Ctrl+C; Claude Code
                    // in a terminal gets the same SIGINT (and keeps running, as after Ctrl+C).
                    if self.runs.contains_key(&id) {
                        self.orch.processes.stopping.insert(pid);
                        self.end_run(&id, Err("stopped".into()), true);
                    } else {
                        runner::interrupt(pid);
                    }
                }
                Some(crate::processes::Event::Stop(pids)) => {
                    self.orch.processes.stopping.extend(pids.iter().copied());
                    crate::processes::terminate(pids);
                }
                None => {}
            }
            return;
        }
        let model = match self.settings.orchestration_model.trim() {
            "" => format!("{} (helper model)", Settings::harness(&self.settings.helper_model, "haiku")),
            m => m.to_owned(),
        };
        let page = orchestration::Page {
            name: &name,
            color: self.workspaces.list[w].color.unwrap_or([128, 128, 128]),
            sessions: &live,
            orch: self.orchestrations.get(&name),
            busy: self.orch.updating.contains_key(&name),
            error: self.orch.errors.get(&name).map(String::as_str),
            asking: self.orch.asking.get(&name).map(|(q, _)| q.as_str()),
            ask_error: self.orch.ask_errors.get(&name).map(String::as_str),
            model: &model,
            arranged: ws.ordered,
        };
        let order: Vec<String> = live.iter().map(|s| s.id.to_owned()).collect();
        let event = orchestration::ui(ui, &page, &mut self.orch.question, &mut self.orch.renaming, &mut self.md_cache);
        match event {
            Some(orchestration::Event::Open(id)) => match self.sessions.iter().position(|s| s.id == id) {
                Some(i) => self.select(i),
                None => self.open_pending(&id),
            },
            Some(orchestration::Event::Update) => self.update_orchestration(w, false),
            Some(orchestration::Event::Clear) => {
                self.orchestrations.refresh();
                self.orchestrations.clear(&name);
                self.orch.ask_errors.remove(&name);
                self.save_orchestrations();
                self.update_orchestration(w, true);
            }
            Some(orchestration::Event::Ask(question)) if !question.is_empty() => {
                let _ = self.ask_orchestrator(w, question);
            }
            Some(orchestration::Event::Forget(i)) => {
                self.orchestrations.refresh();
                let log = &mut self.orchestrations.entry(&name).log;
                if i < log.len() {
                    log.remove(i);
                }
                self.save_orchestrations();
            }
            Some(orchestration::Event::Rename(id, name)) => self.apply(Action::Rename(id, name)),
            Some(orchestration::Event::Move(id, to)) => {
                let mut order = order;
                order.retain(|x| *x != id);
                order.insert(to.min(order.len()), id);
                if let Some(w) = self.sync_workspaces(Some(w)) {
                    self.workspaces.arrange(w, &order);
                    self.save_workspaces();
                }
            }
            Some(orchestration::Event::NewestFirst) => {
                if let Some(w) = self.sync_workspaces(Some(w)) {
                    self.workspaces.unarrange(w);
                    self.save_workspaces();
                }
            }
            Some(orchestration::Event::Ask(_)) | None => {}
        }
    }

    /// Asks workspace `w`'s orchestrator `question`; it may act on it too. Err if one is
    /// already being answered there.
    fn ask_orchestrator(&mut self, w: usize, question: String) -> Result<(), String> {
        let name = self.workspaces.list[w].name.clone();
        if self.orch.asking.contains_key(&name) {
            return Err(format!("the {name} orchestrator is still answering a question"));
        }
        let members = self.orchestration_members(w);
        let bar = self.card_ids(w);
        let harness = Settings::harness(&self.settings.orchestration_harness, orchestration::DEFAULT_HARNESS).to_owned();
        let orch = self.orchestrations.get(&name).cloned();
        let rx = orchestration::ask(self.ctx.clone(), harness, self.settings.orchestration_model.clone(), members, bar, orch, question.clone());
        self.orch.ask_errors.remove(&name);
        self.orch.asking.insert(name, (question, rx));
        Ok(())
    }

    /// `[[id]]` references as the sessions' names, for text read outside the app.
    fn plain_refs(&self, text: &str) -> String {
        orchestration::segments(text)
            .into_iter()
            .map(|(is_ref, piece)| match is_ref {
                true => match self.sessions.iter().position(|s| s.id == piece) {
                    Some(i) => format!("\"{}\"", self.title(i)),
                    None => format!("[[{piece}]]"),
                },
                false => piece.to_owned(),
            })
            .collect()
    }

    /// Commands from `scope …`: done, and their replies written.
    fn poll_control(&mut self) {
        let incoming: Vec<control::Incoming> = self.control.try_iter().collect();
        for c in incoming {
            let reply = c.reply.clone();
            if let Some(result) = self.execute(c.request, Some(c.reply)) {
                control::write_reply(&reply, &result);
            }
        }
    }

    /// The session `s` names: its id, an id prefix (6 or more), or its name (any case). New
    /// sessions whose first prompt is still running count too.
    fn find_session(&self, s: &str) -> Result<String, String> {
        let s = s.trim();
        let pending = self.runs.iter().filter(|(_, r)| r.new).map(|(id, _)| id.as_str());
        let ids: Vec<&str> = self.sessions.iter().map(|m| m.id.as_str()).chain(pending).collect();
        if let Some(id) = ids.iter().find(|id| **id == s) {
            return Ok((*id).to_owned());
        }
        let by_prefix: Vec<&str> = ids.iter().copied().filter(|id| s.len() >= 6 && id.starts_with(s)).collect();
        let by_name: Vec<&str> = ids
            .iter()
            .copied()
            .filter(|id| {
                let name = self.sessions.iter().position(|m| m.id == *id).map(|i| self.title(i)).or_else(|| self.names.get(id));
                name.is_some_and(|n| n.trim().eq_ignore_ascii_case(s))
            })
            .collect();
        match (&by_prefix[..], &by_name[..]) {
            ([id], _) | ([], [id]) => Ok((*id).to_owned()),
            ([], []) => Err(format!("no session is called or has the id \"{s}\" (see scope list)")),
            (many, []) | ([], many) | (many, _) => Err(format!("\"{s}\" could be {} sessions: {}; use the id", many.len(), many.join(", "))),
        }
    }

    /// The workspace a command means: the one named (any case), else the calling session's,
    /// else the open tab's. Ok(None) for no workspace.
    fn find_workspace(&self, name: Option<&str>, caller: Option<&str>) -> Result<Option<usize>, String> {
        let list = &self.workspaces.list;
        if let Some(n) = name.map(str::trim).filter(|n| !n.is_empty()) {
            return list
                .iter()
                .position(|w| w.name.eq_ignore_ascii_case(n))
                .map(Some)
                .ok_or_else(|| format!("no scope is called \"{n}\"; there are: {}", list.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join(", ")));
        }
        let of_caller = caller.and_then(|c| list.iter().position(|w| w.sessions.iter().any(|s| s == c)));
        Ok(of_caller.or(self.tab.filter(|&t| t < list.len())))
    }

    /// Workspace `w`'s sessions as its bar lists them.
    fn visible_in(&self, w: usize) -> Vec<usize> {
        self.bar_order(w, self.workspace_sessions(w))
    }

    /// Ids of workspace `w`'s bar, top first: new sessions still starting, then the listed
    /// ones; in an arranged workspace, all of them where they were put.
    fn bar_ids(&self, w: usize) -> Vec<String> {
        let rows = self.visible_in(w).into_iter().map(|i| (i, None)).collect();
        let pending = pending_new(&self.runs, &self.unlisted, &self.sessions, Some(&self.workspaces.list[w].sessions));
        self.items_in(Some(w), rows, pending)
            .into_iter()
            .map(|item| match item {
                BarItem::Listed(i, _) => self.sessions[i].id.clone(),
                BarItem::Starting(id, _) => id,
            })
            .collect()
    }

    /// What the bar draws for the open tab: `rows` (listed sessions, maybe search results)
    /// and the new sessions still starting.
    fn bar_items(&self, rows: Vec<(usize, Option<String>)>, pending: Vec<(String, String, String)>) -> Vec<BarItem> {
        let arranged = self.search.trim().is_empty().then_some(self.tab).flatten();
        self.items_in(arranged, rows, pending)
    }

    /// Starting sessions (the newest) above the listed ones.
    fn items_in(&self, _w: Option<usize>, rows: Vec<(usize, Option<String>)>, pending: Vec<(String, String, String)>) -> Vec<BarItem> {
        let mut items: Vec<BarItem> = pending.into_iter().map(|(id, title, _)| BarItem::Starting(id, title)).collect();
        items.extend(rows.into_iter().map(|(i, note)| BarItem::Listed(i, note)));
        items
    }

    /// Ids of workspace `w`'s orchestration cards, top first: as arranged by hand, else
    /// in the bar's order.
    fn card_ids(&self, w: usize) -> Vec<String> {
        let mut ids = self.bar_ids(w);
        let ws = &self.workspaces.list[w];
        if ws.ordered {
            // Stable: anything not placed keeps its place after the placed ones.
            ids.sort_by_key(|id| ws.sessions.iter().position(|s| s == id).unwrap_or(usize::MAX));
        }
        ids
    }

    /// A session's name as the bar shows it, for listed and still-starting sessions alike.
    fn name_of(&self, id: &str) -> String {
        if let Some(i) = self.sessions.iter().position(|s| s.id == id) {
            return self.title(i).to_owned();
        }
        let prompt = self.runs.get(id).map(|r| &r.prompt).or_else(|| self.unlisted.iter().find(|u| u.0 == id).map(|u| &u.1));
        match (self.names.get(id), prompt) {
            (Some(n), _) => n.to_owned(),
            (None, Some(p)) => p.lines().next().unwrap_or_default().chars().take(60).collect(),
            (None, None) => id.to_owned(),
        }
    }

    /// Does a command. None when its reply comes later (a question to an orchestrator, whose
    /// reply is written to `reply` when it is answered).
    fn execute(&mut self, request: control::Request, reply: Option<PathBuf>) -> Option<Result<String, String>> {
        let control::Request { command, caller, cwd } = request;
        let caller = caller.as_deref();
        self.sync_workspaces(None);
        Some(match command {
            Command::List { workspace } => self.list_text(workspace.as_deref()),
            Command::Rename { session, name } => self.find_session(&session).map(|id| {
                let before = self.name_of(&id);
                self.apply(Action::Rename(id.clone(), name.clone()));
                format!("Renamed \"{before}\" to \"{}\"", self.name_of(&id))
            }),
            Command::Move { session, to } => self.find_session(&session).and_then(|id| {
                let w = self.workspaces.list.iter().position(|w| w.sessions.contains(&id)).ok_or("that session is in no scope, so it has no card to move")?;
                let mut order = self.card_ids(w);
                let from = order.iter().position(|x| *x == id).ok_or("that session has no card in its scope")?;
                let at = control::place(&to, from, order.len())?;
                order.remove(from);
                order.insert(at, id.clone());
                self.workspaces.arrange(w, &order);
                self.save_workspaces();
                Ok(format!("Moved \"{}\" to place {} of {} on the {} orchestration page", self.name_of(&id), at + 1, order.len(), self.workspaces.list[w].name))
            }),
            Command::New { prompt, workspace, folder, name } => self.find_workspace(workspace.as_deref(), caller).and_then(|w| {
                let dir = self.new_session_folder(w, folder, cwd)?;
                let id = runner::new_session_id();
                if let Some(w) = w {
                    self.workspaces.move_to(&id, w);
                    self.save_workspaces();
                }
                if let Some(n) = name.as_deref().filter(|n| !n.trim().is_empty()) {
                    self.apply(Action::Rename(id.clone(), n.to_owned()));
                }
                self.start_run(id.clone(), Some(dir.clone()), prompt, true);
                let place = w.map_or(String::new(), |w| format!(" in {}", self.workspaces.list[w].name));
                let called = name.map_or(String::new(), |n| format!(" \"{}\"", n.trim()));
                Ok(format!("Started session{called}{place} in {} (id {id})", dir.display()))
            }),
            Command::Style { args } => {
                let changed = self.settings.change(|s| crate::style::command(s, &args));
                match changed {
                    Ok(r) => {
                        self.apply_settings();
                        r
                    }
                    Err(e) => Err(format!("could not save settings: {e}")),
                }
            }
            Command::Ask { question, workspace } => {
                let asked = self.find_workspace(workspace.as_deref(), caller).and_then(|w| {
                    let w = w.ok_or("which scope? none is open; use --scope")?;
                    self.ask_orchestrator(w, question)?;
                    Ok(self.workspaces.list[w].name.clone())
                });
                match (asked, reply) {
                    (Ok(name), Some(path)) => {
                        self.orch.asked_by.insert(name, path);
                        return None;
                    }
                    (Ok(name), None) => Ok(format!("Asked the {name} orchestrator")),
                    (Err(e), _) => Err(e),
                }
            }
        })
    }

    /// Where a new session starts: the folder asked for, else its workspace's (picked for it,
    /// else that of its newest session), else the caller's.
    fn new_session_folder(&self, w: Option<usize>, folder: Option<PathBuf>, cwd: Option<PathBuf>) -> Result<PathBuf, String> {
        if let Some(f) = folder {
            let f = if f.is_relative() { cwd.clone().unwrap_or_default().join(f) } else { f };
            return if f.is_dir() { Ok(f) } else { Err(format!("{} is not a folder", f.display())) };
        }
        let of_workspace = w.and_then(|w| {
            let picked = self.workspaces.list[w].folder.clone().filter(|d| d.is_dir());
            picked.or_else(|| self.workspace_sessions(w).into_iter().find_map(|i| session::load(&self.sessions[i].path).ok()?.cwd.filter(|d| d.is_dir())))
        });
        of_workspace.or(cwd.filter(|d| d.is_dir())).ok_or_else(|| "which folder? the scope has none yet; use --folder".to_owned())
    }

    /// `scope list`: each workspace (or the one asked for) with its sessions in bar order.
    fn list_text(&self, workspace: Option<&str>) -> Result<String, String> {
        let only = match workspace {
            Some(n) => Some(self.find_workspace(Some(n), None)?.unwrap_or_default()),
            None => None,
        };
        let mut out = String::new();
        for (w, ws) in self.workspaces.list.iter().enumerate().filter(|(w, _)| only.is_none_or(|o| o == *w)) {
            let order = if ws.bar.is_empty() { "newest first" } else { "arranged by hand" };
            out.push_str(&format!("{} ({order})\n", ws.name));
            for (k, id) in self.bar_ids(w).into_iter().enumerate() {
                let listed = self.sessions.iter().position(|s| s.id == id);
                let state = match (self.runs.contains_key(&id), listed) {
                    (_, None) => "  [starting]".to_owned(),
                    (true, Some(_)) => "  [working]".to_owned(),
                    (false, Some(i)) => self.verdict_of(i).map_or(String::new(), |v| format!("  [{}]", v.label().split_once(' ').map_or("", |(_, word)| word))),
                };
                out.push_str(&format!("  {}. {}  {id}{state}\n", k + 1, self.name_of(&id)));
            }
        }
        if out.is_empty() {
            out.push_str("No scopes yet.\n");
        }
        Ok(out.trim_end().to_owned())
    }

    /// The main area shows the workspaces overview: All sessions with no session open.
    fn showing_overview(&self) -> bool {
        self.tab.is_none() && self.open.is_none() && !(self.settings_active && self.settings_tab)
    }

    fn poll_overviews(&mut self) {
        let mut done = Vec::new();
        for (name, rx) in &self.overview_jobs {
            match rx.try_recv() {
                Ok(result) => done.push((name.clone(), result)),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => done.push((name.clone(), Err("stopped unexpectedly".into()))),
            }
        }
        for (name, result) in done {
            self.overview_jobs.remove(&name);
            match result {
                Ok(s) => {
                    self.overview_errors.remove(&name);
                    if let Err(e) = self.overviews.change(|o| o.set(&name, s)) {
                        self.error = Some(format!("could not save the overview: {e}"));
                    }
                }
                Err(e) => {
                    self.overview_errors.insert(name, e);
                }
            }
        }
    }

    /// A workspace's sessions, newest first, as indices into `sessions`.
    fn workspace_sessions(&self, w: usize) -> Vec<usize> {
        let ws = &self.workspaces.list[w];
        (0..self.sessions.len()).filter(|&i| ws.sessions.contains(&self.sessions[i].id)).collect()
    }

    /// Starts summarizing workspace `w` with the helper model, unless it already is.
    fn summarize_workspace(&mut self, w: usize) {
        let name = self.workspaces.list[w].name.clone();
        let members = self.workspace_sessions(w);
        if members.is_empty() || self.overview_jobs.contains_key(&name) {
            return;
        }
        let newest = overview::secs(self.sessions[members[0]].modified);
        let sources = self.overview_sources(&members);
        let previous = self.overviews.get(&name).cloned();
        self.overview_errors.remove(&name);
        self.overview_jobs.insert(name, overview::start(self.ctx.clone(), sources, previous, newest));
    }

    /// Sessions (indices into `sessions`) as the overview summarizer reads them.
    fn overview_sources(&self, members: &[usize]) -> Vec<overview::Source> {
        members
            .iter()
            .map(|&i| {
                let s = &self.sessions[i];
                overview::Source {
                    id: s.id.clone(),
                    title: self.title(i).to_owned(),
                    goal: self.goals.get(&s.id).map(str::to_owned),
                    path: s.path.clone(),
                    modified: overview::secs(s.modified),
                    running: self.runs.contains_key(&s.id),
                }
            })
            .collect()
    }

    /// The overview in the main area. Missing or stale summaries are written as it shows,
    /// a few at a time.
    fn overview(&mut self, ui: &mut egui::Ui) {
        const AT_ONCE: usize = 3;
        let now = overview::secs(std::time::SystemTime::now());
        let members: Vec<Vec<usize>> = (0..self.workspaces.list.len()).map(|w| self.workspace_sessions(w)).collect();
        for (w, m) in members.iter().enumerate() {
            let name = &self.workspaces.list[w].name;
            let wanted = !self.overview_errors.contains_key(name) && overview::needs_summary(self.overviews.get(name), &self.overview_sources(m), now);
            if wanted && self.overview_jobs.len() < AT_ONCE {
                self.summarize_workspace(w);
            }
        }
        // Checked again every minute, for "last interaction" and summaries to bring up to date.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(60));
        let mut cards: Vec<overview::Card> = self
            .workspaces
            .list
            .iter()
            .zip(&members)
            .enumerate()
            .map(|(w, (ws, m))| {
                let ids = || m.iter().map(|&i| &self.sessions[i].id);
                let rows = m
                    .iter()
                    .map(|&i| {
                        let s = &self.sessions[i];
                        overview::Row {
                            id: &s.id,
                            title: self.title(i),
                            modified: overview::secs(s.modified),
                            running: self.runs.contains_key(&s.id),
                            tool: self.runs.get(&s.id).and_then(|r| r.tool.as_deref()),
                            queued: self.queues.get(&s.id).map_or(0, |q| q.items.len()),
                            finished: self.finished_unseen(i),
                        }
                    })
                    .collect();
                overview::Card {
                    w,
                    name: &ws.name,
                    color: ws.color.unwrap_or([128, 128, 128]),
                    running: ids().filter(|id| self.runs.contains_key(*id)).count(),
                    queued: ids().filter_map(|id| self.queues.get(id)).map(|q| q.items.len()).sum(),
                    rows,
                    summary: self.overviews.get(&ws.name),
                    busy: self.overview_jobs.contains_key(&ws.name),
                    error: self.overview_errors.get(&ws.name).map(String::as_str),
                }
            })
            .collect();
        overview::sort(&mut cards);
        let grouped: HashSet<&String> = self.workspaces.list.iter().flat_map(|w| &w.sessions).collect();
        let loose = self.sessions.iter().filter(|s| !grouped.contains(&s.id)).count();
        let opts = overview::Options { loose, jira_url: &self.settings.jira_url, stale_after: u64::from(self.settings.stale_after_hours) * 3600 };
        let event = overview::ui(ui, &cards, &opts);
        match event {
            Some(overview::Event::Open(w)) => self.switch_tab(Some(w)),
            Some(overview::Event::OpenSession(w, id)) => {
                if let Some(i) = self.sessions.iter().position(|s| s.id == id) {
                    if self.tab != Some(w) {
                        self.search.clear();
                    }
                    self.tab = Some(w);
                    self.apply(Action::Select(i));
                }
            }
            Some(overview::Event::Finish(id)) => self.apply(Action::Remove(vec![id], None)),
            Some(overview::Event::Refresh(w)) => self.summarize_workspace(w),
            Some(overview::Event::RefreshAll) => {
                for w in 0..self.workspaces.list.len() {
                    self.summarize_workspace(w);
                }
            }
            None => {}
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
            self.sync_workspaces(None);
            self.workspaces.chose_folder(self.tab, &dir);
            self.save_workspaces();
            self.start_new_session(dir);
        }
    }

    /// Empty session in `dir`; the first prompt creates it.
    fn start_new_session(&mut self, dir: PathBuf) {
        self.switch_view(Some(format!("new:{}", dir.display())));
        self.selected = None;
        self.orchestrating = false;
        self.new_id = None;
        self.open = Some(Session { cwd: Some(dir.clone()), ..Default::default() });
        self.new_session = Some(dir);
        self.new_info = None;
        self.new_choice = runner::Choice::default();
        self.history = History::default();
        self.pinned = None;
        self.picked_message = None;
        self.top_message = None;
        self.inline_you = false;
        self.error = None;
        self.focus_prompt = true;
    }

    fn open_pending(&mut self, id: &str) {
        let Some(run) = self.runs.get(id) else { return };
        let (cwd, prompt) = (run.cwd.clone(), run.prompt.clone());
        self.switch_view(Some(id.to_owned()));
        self.selected = None;
        self.orchestrating = false;
        self.new_id = Some(id.to_owned());
        let mut s = Session { cwd: cwd.clone(), ..Default::default() };
        s.push_prompt(prompt);
        self.open = Some(s);
        self.new_session = cwd.or_else(|| Some(PathBuf::from(".")));
        self.pinned = None;
        self.picked_message = None;
        self.top_message = None;
        self.inline_you = false;
        self.heights.clear();
    }

    /// Every session's pending answers, so one asked before switching away still arrives.
    fn poll_btw(&mut self) {
        for b in self.btw.values_mut().flatten() {
            if let Some(Ok(result)) = b.rx.as_ref().map(|rx| rx.try_recv()) {
                b.answer = Some(result);
                b.rx = None;
            }
        }
    }

    fn poll_summary(&mut self) {
        let Some((id, rx)) = &self.summarizing else { return };
        if let Ok(result) = rx.try_recv() {
            let id = id.clone();
            self.summarizing = None;
            match result {
                Ok(goal) => {
                    if let Err(e) = self.goals.change(|g| g.set(&id, &goal)) {
                        self.error = Some(format!("could not save goal: {e}"));
                    }
                }
                Err(e) => self.error = Some(format!("could not summarize: {e}")),
            }
        }
    }

    /// Starts judging whether session `id`'s work looks finished, unless it already is being
    /// judged (or isn't listed yet: the sweep gets to it then).
    fn judge(&mut self, id: &str) {
        if !self.settings.tag_finished || self.judging.iter().any(|(j, _)| j == id) {
            return;
        }
        let Some(i) = self.sessions.iter().position(|s| s.id == id) else { return };
        let rx = verdict::check(self.ctx.clone(), self.title(i).to_owned(), self.sessions[i].path.clone());
        self.judging.push((id.to_owned(), rx));
    }

    /// Takes finished verdicts. Every few seconds, starts checks (a couple at a time) for
    /// workspace sessions with recent activity no verdict has read, such as runs that ended
    /// while scope was closed or ran in a terminal.
    fn poll_verdicts(&mut self) {
        const QUIET: u64 = 60;
        const EVERY: u64 = 10;
        const WINDOW: u64 = 3 * 24 * 3600;
        const AT_ONCE: usize = 2;
        let mut done = Vec::new();
        self.judging.retain(|(id, rx)| match rx.try_recv() {
            Ok(r) => {
                done.push((id.clone(), r));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        for (id, result) in done {
            match result {
                Ok(v) => {
                    self.judge_failed.remove(&id);
                    if let Err(e) = self.verdicts.change(|all| all.set(&id, v)) {
                        self.error = Some(format!("could not save the session tags: {e}"));
                    }
                }
                Err(_) => {
                    let modified = self.sessions.iter().find(|s| s.id == id).map_or(0, |s| overview::secs(s.modified));
                    self.judge_failed.insert(id, modified);
                }
            }
        }
        if !self.settings.tag_finished || self.judging.len() >= AT_ONCE || self.last_sweep.is_some_and(|t| t.elapsed().as_secs() < EVERY) {
            return;
        }
        self.last_sweep = Some(std::time::Instant::now());
        self.ctx.request_repaint_after(std::time::Duration::from_secs(EVERY));
        let in_workspace: HashSet<&str> = self.workspaces.list.iter().flat_map(|w| w.sessions.iter().map(String::as_str)).collect();
        let sessions: Vec<(String, u64, bool)> = self
            .sessions
            .iter()
            .filter(|s| in_workspace.contains(s.id.as_str()))
            .map(|s| (s.id.clone(), overview::secs(s.modified), self.runs.contains_key(&s.id) || self.judging.iter().any(|(j, _)| *j == s.id)))
            .collect();
        let now = overview::secs(SystemTime::now());
        let free = AT_ONCE.saturating_sub(self.judging.len());
        for (id, _) in verdict::candidates(&sessions, &self.verdicts, &self.judge_failed, now, QUIET, WINDOW).into_iter().take(free) {
            self.judge(&id);
        }
    }

    /// The tag a session shows for how its work ended, if a verdict has read its newest change.
    fn verdict_of(&self, i: usize) -> Option<&Verdict> {
        let s = &self.sessions[i];
        if self.runs.contains_key(&s.id) {
            return None;
        }
        self.verdicts.current(&s.id, overview::secs(s.modified))
    }

    /// Names written for new sessions. One renamed meanwhile keeps its name; a failure only
    /// leaves the first prompt as the name, so it isn't reported.
    fn poll_naming(&mut self) {
        let mut written = Vec::new();
        self.naming.retain(|(id, rx)| match rx.try_recv() {
            Ok(result) => {
                if let Some(name) = result.ok().and_then(|r| names::clean(&r)) {
                    written.push((id.clone(), name));
                }
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        if written.is_empty() {
            return;
        }
        // Renamed meanwhile also counts a rename made in another scope.
        let saved = self.names.change(|n| {
            for (id, name) in &written {
                if n.get(id).is_none() {
                    n.set(id, name);
                }
            }
        });
        if let Err(e) = saved {
            self.error = Some(format!("could not save names: {e}"));
        }
    }

    fn reload(&mut self) {
        for tab in &mut self.file_tabs {
            tab.contents = diffview::read(&tab.path);
        }
        let Some(meta) = self.selected.and_then(|i| self.sessions.get(i)) else { return };
        match session::load(&meta.path) {
            Ok(mut s) => {
                images::save_all(&mut s.images);
                self.history = History::new(s.prompts.clone());
                self.open = Some(s);
                self.pinned = None;
                self.picked_message = None;
                self.top_message = None;
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
        self.sync_workspaces(None);
        let current = self.selected.map(|i| (self.sessions[i].id.clone(), self.sessions[i].modified));
        self.sessions = snap.sessions;
        self.index = snap.index;
        let sessions = &self.sessions;
        self.unlisted.retain(|(id, ..)| !sessions.iter().any(|s| &s.id == id));
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
            if !self.new_choice.is_empty() {
                self.choices.insert(id.clone(), self.new_choice.clone());
            }
            self.new_id = Some(id.clone());
            self.sync_workspaces(None);
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
        let choice = self.choices.get(&id).cloned().unwrap_or_default();
        let (rx, stop) = runner::send(self.ctx.clone(), id.clone(), cwd.clone(), prompt.clone(), new, choice);
        self.runs.insert(id, Run { rx, live: String::new(), tool: None, new, cwd, prompt, stop, poked: false, answer: String::new() });
    }

    /// Takes in prompt runs this scope did not start, such as the ones still going from before a
    /// restart: they show as working, their steps arrive as Claude saves them, and Stop, the
    /// queue and notifications work as for scope's own runs.
    fn adopt_runs(&mut self) {
        let Some(found) = self.foreign_runs.try_iter().last() else { return };
        for (id, pid) in found {
            if self.runs.contains_key(&id) {
                continue;
            }
            let Some(path) = self.sessions.iter().find(|s| s.id == id).map(|s| s.path.clone()) else { continue };
            let s = session::load(&path).unwrap_or_default();
            let prompt = s.prompts.last().cloned().unwrap_or_default();
            let (rx, stop) = runner::follow(self.ctx.clone(), pid, path);
            self.runs.insert(id.clone(), Run { rx, live: String::new(), tool: None, new: false, cwd: s.cwd, prompt, stop, poked: false, answer: String::new() });
            // Following starts at the file's end: the open copy catches up to it first.
            if self.new_session.is_none() && self.open_id() == Some(id) {
                self.reload();
            }
        }
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
                        if run.new && v["type"] == "assistant" && run.answer.len() < 4000 {
                            for text in v["message"]["content"].as_array().into_iter().flatten().filter_map(|c| c["text"].as_str()) {
                                run.answer.push_str(text);
                                run.answer.push('\n');
                            }
                        }
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
        if run.new && !self.sessions.iter().any(|s| s.id == id) {
            self.unlisted.push((id.to_owned(), run.prompt.clone(), run.cwd.clone()));
        }
        // After the first answer, the helper model names the session (unless it has a name).
        if run.new && !stopped && result.is_ok() && self.names.get(id).is_none() {
            self.naming.push((id.to_owned(), runner::name_session(self.ctx.clone(), &run.prompt, &run.answer)));
        }
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
            runner::notify("scope", &body);
        }
        // Done, or waiting for the developer? The helper model reads how it ended.
        if !stopped && result.is_ok() && !self.runs.contains_key(id) {
            self.judge(id);
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

    /// Takes the usage endpoint's latest answer, and tells it whether Claude is working, so it
    /// asks every 10 s then and every 10 minutes otherwise. A failure keeps the last numbers.
    fn poll_usage(&mut self) {
        self.usage_busy.store(!self.runs.is_empty(), std::sync::atomic::Ordering::Relaxed);
        for answer in self.usage_rx.try_iter() {
            match answer {
                Ok(windows) => {
                    self.usage = Some((windows, SystemTime::now()));
                    self.usage_error = None;
                }
                Err(e) => self.usage_error = Some(e),
            }
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

    /// Pasted or dropped images: their markers go in at the cursor, a space after them to go on
    /// typing. Ones that couldn't be read show as the error above the input.
    fn insert_images(&mut self, added: Vec<Result<String, String>>) {
        let mut markers = Vec::new();
        for a in added {
            match a {
                Ok(m) => markers.push(m),
                Err(e) => self.error = Some(e),
            }
        }
        if markers.is_empty() {
            return;
        }
        let text = markers.join(" ") + " ";
        let id = Id::new("prompt-input");
        let chars = self.input.chars().count();
        let at = egui::TextEdit::load_state(&self.ctx, id).and_then(|s| s.cursor.char_range()).map_or(chars, |r| usize::from(r.primary.index).min(chars));
        let (input, end) = dictation::insert_at(&self.input, at, &text);
        let rest = input.chars().nth(end);
        // insert_at trims: the space goes back unless one follows already.
        self.input = if rest.is_some_and(char::is_whitespace) { input } else { insert_char(&input, end, ' ') };
        let mut state = egui::TextEdit::load_state(&self.ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(end + 1))));
        state.store(&self.ctx, id);
        self.focus_prompt = true;
    }

    /// Sessions in the active tab (all, or one workspace), newest first.
    fn visible(&self) -> Vec<usize> {
        match self.tab.filter(|&w| w < self.workspaces.list.len()) {
            None => (0..self.sessions.len()).collect(),
            Some(w) => self.visible_in(w),
        }
    }

    /// Workspace `w`'s sessions (indices into `sessions`) in the order its bar lists them:
    /// newest first, by the time each row shows, unless its rows were dragged into an order;
    /// then as placed, below any not placed yet (an arrangement of the orchestration page's
    /// cards orders only them).
    fn bar_order(&self, w: usize, mut sessions: Vec<usize>) -> Vec<usize> {
        sessions.sort_by_key(|&i| std::cmp::Reverse(self.sessions[i].modified));
        if let Some(bar) = self.workspaces.list.get(w).map(|ws| &ws.bar).filter(|b| !b.is_empty()) {
            sessions.sort_by_key(|&i| bar.iter().position(|id| *id == self.sessions[i].id).map_or(0, |p| p + 1));
        }
        sessions
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
        let harness = Settings::harness(&self.settings.search_harness, search::DEFAULT_HARNESS);
        let rx = runner::ask_haiku(ctx.clone(), search::ai_prompt(harness, &query, &catalog));
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
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.search)
                .id(Id::new("search"))
                .hint_text("🔍 Search · Enter asks Haiku")
                .desired_width(ui.available_width());
            let resp = ui.add(edit);
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.search.clear();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.ask_ai(ui.ctx());
                resp.request_focus();
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
        let search_rect = egui::Panel::bottom("search")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
            .resizable(false)
            .show(ui, |ui| self.search_bar(ui))
            .response
            .rect;

        let rows = self.rows();
        let members = self.tab.and_then(|w| self.workspaces.list.get(w)).map(|w| w.sessions.as_slice());
        let pending = pending_new(&self.runs, &self.unlisted, &self.sessions, members);
        if rows.is_empty() && pending.is_empty() && !self.search.trim().is_empty() {
            ui.label(RichText::new("No matches. Press Enter to ask Haiku.").weak());
        } else if rows.is_empty() && pending.is_empty() {
            ui.label(if self.tab.is_some() {
                "Empty scope. Drag sessions onto its tab, or right-click a session and choose Move to."
            } else {
                "No sessions found in ~/.claude/projects"
            });
        }
        let order: Vec<String> = rows.iter().map(|&(i, _)| self.sessions[i].id.clone()).collect();
        let items = self.bar_items(rows, pending);
        // Rows picked, in list order: what a drag or the context menu acts on.
        let picked: Vec<String> = order.iter().filter(|id| self.picked.contains(*id)).cloned().collect();
        let open_id = self.selected.map(|i| self.sessions[i].id.clone());
        // Highlighted rows, what the keys act on: the picked ones, or else the open one.
        let highlighted: Vec<String> = if picked.is_empty() { open_id.iter().filter(|id| order.contains(id)).cloned().collect() } else { picked.clone() };
        // A press anywhere takes the keyboard from the bar; a click on a session gives it back.
        if ui.input(|i| i.pointer.any_pressed()) {
            self.bar_keys = false;
        }
        let mut bar_keys = false;
        let modal = !self.confirm_trash.is_empty() || self.confirm_delete.is_some();
        if self.bar_keys && !modal && ui.memory(|m| m.focused().is_none()) {
            let (up, down, delete, esc, typed) = ui.input_mut(|i| {
                let typed: String = i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.as_str()) } else { None }).collect();
                let up = i.consume_key(Modifiers::NONE, Key::ArrowUp);
                let down = i.consume_key(Modifiers::NONE, Key::ArrowDown);
                let delete = i.consume_key(Modifiers::NONE, Key::Delete) | i.consume_key(Modifiers::NONE, Key::Backspace);
                (up, down, delete, i.key_pressed(Key::Escape), typed)
            });
            if up || down {
                let at = open_id.as_ref().and_then(|id| order.iter().position(|o| o == id));
                let next = match at {
                    Some(k) if up => k.checked_sub(1),
                    Some(k) => Some(k + 1).filter(|&n| n < order.len()),
                    None => (!order.is_empty()).then_some(0),
                };
                if let Some(i) = next.and_then(|k| self.sessions.iter().position(|s| s.id == order[k])) {
                    actions.push(Action::Select(i));
                }
            }
            // Only a workspace's own sessions can leave it; All sessions has nothing to take them out of.
            if delete && self.tab.is_some() && !highlighted.is_empty() {
                actions.push(Action::Remove(highlighted.clone(), after_remove(&order, &highlighted)));
            }
            bar_keys = up || down || delete;
            if esc || !typed.is_empty() {
                self.bar_keys = false;
                bar_keys = false;
                self.type_into_prompt(ui.ctx(), &typed);
            }
        }
        let (modifiers, press, pointer, primary_down, released) =
            ui.input(|i| (i.modifiers, i.pointer.press_origin(), i.pointer.interact_pos(), i.pointer.primary_down(), i.pointer.any_released()));
        // Inside a scope, with no search, dragging rows up or down reorders the bar.
        let reorderable = self.tab.is_some() && self.search.trim().is_empty();
        let arranged = self.tab.and_then(|w| self.workspaces.list.get(w)).is_some_and(|w| !w.bar.is_empty());
        // Where a press keeps the search: its rows, as far as they show.
        let mut results = Vec::new();
        egui::ScrollArea::vertical().show(ui, |ui| {
            // The list's top-left, which moves with the scroll.
            let top = ui.min_rect().min;
            let mut rects = Vec::new();
            // The workspace's own bubble, above its sessions: its orchestration page.
            if let Some(ws) = self.tab.and_then(|w| self.workspaces.list.get(w)) {
                let ids = self.workspace_sessions(self.tab.unwrap_or_default());
                let working = ids.iter().filter(|&&i| self.runs.contains_key(&self.sessions[i].id)).count();
                let mut line = format!("{} session{}", ids.len(), if ids.len() == 1 { "" } else { "s" });
                if working > 0 {
                    line.push_str(&format!(" · {working} working"));
                }
                let report = self.orchestrations.get(&ws.name).and_then(|o| o.report.as_ref());
                if let Some(r) = report {
                    line.push_str(&format!(" · {}%", r.progress));
                    let waiting = r.sessions.iter().filter(|s| s.state == "waiting" || s.state == "blocked").count();
                    if waiting > 0 {
                        line.push_str(&format!(" · {waiting} need{} you", if waiting == 1 { "s" } else { "" }));
                    }
                }
                let busy = working > 0 || self.orch.updating.contains_key(&ws.name);
                if orchestration::bubble(ui, self.showing_orchestration(), &line, busy).clicked() {
                    actions.push(Action::Orchestrate);
                }
            }
            for item in items {
                let (i, note) = match item {
                    BarItem::Listed(i, note) => (i, note),
                    BarItem::Starting(id, title) => {
                        let open = self.new_session.is_some() && self.new_id.as_ref() == Some(&id);
                        let title = self.names.get(&id).unwrap_or(&title);
                        if session_bubble(ui, open, title, None, None, true, None, None).clicked() && !open {
                            actions.push(Action::OpenPending(id));
                        }
                        continue;
                    }
                };
                let s = &self.sessions[i];
                let title = self.names.get(&s.id).unwrap_or(&s.title);
                if let Some(r) = self.renaming.as_mut().filter(|r| r.id == s.id) {
                    let field = ui.scope(|ui| rename_field(ui, r, &s.project, &mut actions)).response.rect;
                    results.push(field.intersect(ui.clip_rect()));
                    continue;
                }
                let queued = match self.queues.get(&s.id).map(|q| q.items.len()) {
                    Some(n) if n > 0 => format!("  +{n} queued"),
                    _ => String::new(),
                };
                let label = format!("{title}{queued}");
                let color = self.tab.is_none().then(|| self.workspaces.color_of(&s.id)).flatten();
                let is_picked = self.picked.contains(&s.id);
                let highlighted = is_picked || (self.picked.is_empty() && self.selected == Some(i));
                let when = row_time(s.modified, SystemTime::now());
                let tag = self.verdict_of(i);
                let row = session_bubble(ui, highlighted, &label, note.as_deref(), Some(&when), self.runs.contains_key(&s.id), color, tag);
                rects.push((s.id.clone(), row.rect));
                results.push(row.rect.intersect(ui.clip_rect()));
                // Dragging highlighted rows moves them; a drag from any other row draws a box.
                let dragging = if is_picked { picked.clone() } else { vec![s.id.clone()] };
                if row.drag_started() {
                    if highlighted {
                        row.dnd_set_drag_payload(dragging.clone());
                    } else if let Some(p) = press {
                        let base = if modifiers.shift || modifiers.command { self.picked.clone() } else { HashSet::new() };
                        self.lasso = Some((p - top, base));
                    }
                }
                if row.dragged() && self.lasso.is_none() {
                    if let Some(pos) = ui.ctx().pointer_interact_pos() {
                        let painter = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::new("drag-ghost")));
                        let font = egui::FontId::proportional(14.0);
                        let text = if dragging.len() > 1 { format!("{} sessions", dragging.len()) } else { title.to_owned() };
                        painter.text(pos + egui::vec2(14.0, 0.0), Align2::LEFT_CENTER, text, font, theme::palette(ui.visuals()).user);
                    }
                }
                if row.clicked() {
                    bar_keys = true;
                    if modifiers.shift {
                        // A range from the session last clicked (or the open one) to this one.
                        let from = self.pick_anchor.as_ref().or(open_id.as_ref()).and_then(|a| order.iter().position(|id| id == a));
                        let to = order.iter().position(|id| id == &s.id).unwrap_or(0);
                        let (a, b) = from.map_or((to, to), |f| (f.min(to), f.max(to)));
                        let anchor = self.pick_anchor.clone().unwrap_or_else(|| s.id.clone());
                        actions.push(Action::Pick(order[a..=b].iter().cloned().collect(), anchor));
                    } else if modifiers.command {
                        let mut ids = self.picked.clone();
                        if ids.is_empty() {
                            ids.extend(open_id.iter().filter(|id| order.contains(id)).cloned());
                        }
                        if !ids.remove(&s.id) {
                            ids.insert(s.id.clone());
                        }
                        actions.push(Action::Pick(ids, s.id.clone()));
                    } else {
                        actions.push(Action::Select(i));
                    }
                }
                if row.double_clicked() && !modifiers.shift && !modifiers.command {
                    self.renaming = Some(Renaming { id: s.id.clone(), text: title.to_owned(), fresh: true });
                }
                row.context_menu(|ui| {
                    let n = dragging.len();
                    if n > 1 {
                        ui.label(RichText::new(format!("{n} sessions")).weak());
                    }
                    ui.menu_button("Move to", |ui| {
                        if self.workspaces.list.is_empty() {
                            ui.label("No scopes yet. Create one with +");
                        }
                        for (w, ws) in self.workspaces.list.iter().enumerate() {
                            if ui.button(workspaces::label(&ws.name)).clicked() {
                                actions.push(Action::MoveTo(dragging.clone(), w));
                            }
                        }
                    });
                    if self.tab.is_some() && ui.button("Remove from scope").clicked() {
                        // Taking the open session out opens its neighbor, as Delete does.
                        let then = open_id.as_ref().filter(|id| dragging.contains(id)).and_then(|_| after_remove(&order, &dragging));
                        actions.push(Action::Remove(dragging.clone(), then));
                    }
                    if arranged && ui.button("Sort newest first").on_hover_text("Undo the order the rows were dragged into").clicked() {
                        actions.push(Action::BarNewestFirst);
                    }
                    ui.separator();
                    let delete = if n > 1 { format!("Move {n} sessions to Trash…") } else { "Move to Trash…".to_owned() };
                    if ui.button(delete).clicked() {
                        actions.push(Action::Trash(dragging.clone()));
                    }
                });
            }
            // Rows dragged within the list: a line shows where they go, letting go puts them there.
            if reorderable
                && let Some(pos) = pointer
                && let Some(moving) = egui::DragAndDrop::payload::<Vec<String>>(ui.ctx())
                && ui.clip_rect().contains(pos)
                && !rects.is_empty()
            {
                let ids: Vec<String> = rects.iter().map(|(id, _)| id.clone()).collect();
                let slot = rects.iter().filter(|(_, r)| r.center().y < pos.y).count();
                let gap = ui.spacing().item_spacing.y / 2.0;
                let y = rects.get(slot).map_or_else(|| rects[rects.len() - 1].1.bottom() + gap, |(_, r)| r.top() - gap);
                let x = rects[0].1.x_range();
                ui.painter().hline(x, y, Stroke::new(2.0, theme::palette(ui.visuals()).user));
                if released {
                    egui::DragAndDrop::clear_payload(ui.ctx());
                    let order = dropped_order(&ids, &moving, slot);
                    if order != ids {
                        actions.push(Action::ArrangeBar(order));
                    }
                } else {
                    // Near the top or bottom edge, the list scrolls to reach more rows.
                    let clip = ui.clip_rect();
                    let step = if pos.y < clip.top() + 16.0 { 8.0 } else if pos.y > clip.bottom() - 16.0 { -8.0 } else { 0.0 };
                    if step != 0.0 {
                        ui.scroll_with_delta(egui::vec2(0.0, step));
                    }
                    ui.ctx().request_repaint();
                }
            }
            // The space under the rows: a drag there draws a box, a click drops the picks.
            let below = egui::Rect::from_min_max(ui.cursor().min, egui::pos2(ui.max_rect().right(), ui.clip_rect().bottom()));
            if below.height() > 0.0 {
                let space = ui.interact(below, Id::new("session-list-space"), Sense::click_and_drag());
                if space.drag_started()
                    && let Some(p) = press
                {
                    let base = if modifiers.shift || modifiers.command { self.picked.clone() } else { HashSet::new() };
                    self.lasso = Some((p - top, base));
                }
                if space.clicked() && !modifiers.shift && !modifiers.command {
                    self.picked.clear();
                }
            }
            if let Some((start, base)) = &self.lasso {
                match pointer.filter(|_| primary_down) {
                    None => self.lasso = None,
                    Some(pos) => {
                        let rect = egui::Rect::from_two_pos(top + *start, pos);
                        let pal = theme::palette(ui.visuals());
                        ui.painter().rect(rect, 2.0, pal.user.gamma_multiply(0.12), Stroke::new(1.0, pal.user), StrokeKind::Inside);
                        let mut ids = base.clone();
                        ids.extend(rects.iter().filter(|(_, r)| r.intersects(rect)).map(|(id, _)| id.clone()));
                        self.picked = ids;
                        // Near the top or bottom edge, the list scrolls to reach more rows.
                        let clip = ui.clip_rect();
                        let step = if pos.y < clip.top() + 16.0 {
                            8.0
                        } else if pos.y > clip.bottom() - 16.0 {
                            -8.0
                        } else {
                            0.0
                        };
                        if step != 0.0 {
                            ui.scroll_with_delta(egui::vec2(0.0, step));
                            ui.ctx().request_repaint();
                        }
                    }
                }
            }
        });
        // A press anywhere but the search field and its results ends the search, back to the
        // sessions the bar shows without one. A menu opened from a result keeps it.
        if !self.search.trim().is_empty()
            && ui.input(|i| i.pointer.any_pressed())
            && !egui::Popup::is_any_open(ui.ctx())
            && let Some(p) = press
            && !search_rect.contains(p)
            && !results.iter().any(|r| r.contains(p))
        {
            self.search.clear();
        }
        for a in actions {
            self.apply(a);
        }
        // A session clicked, or the keys moved in the bar: it keeps the keyboard (opening a
        // session would otherwise send it to the input).
        if bar_keys {
            self.bar_keys = true;
            self.focus_prompt = false;
            ui.memory_mut(|m| m.stop_text_input());
        }
    }

    /// Gives the input the keyboard, adding `typed` (keys pressed elsewhere) at its end.
    fn type_into_prompt(&mut self, ctx: &egui::Context, typed: &str) {
        self.focus_prompt = true;
        if typed.is_empty() {
            return;
        }
        self.input.push_str(typed);
        let id = Id::new("prompt-input");
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(self.input.chars().count()))));
        state.store(ctx, id);
    }

    fn tabs(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal_wrapped(|ui| {
            let all = ui.selectable_label(self.tab.is_none(), "SCOPES").on_hover_text("Every session, and an overview of the scopes");
            if all.clicked() {
                actions.push(Action::Tab(None));
            }
            for (w, ws) in self.workspaces.list.iter().enumerate() {
                if let Some((_, text, fresh)) = self.workspace_rename.as_mut().filter(|r| r.0 == w) {
                    workspace_rename_field(ui, w, text, fresh, actions);
                    continue;
                }
                let tab = workspaces::tab(ui, &ws.name, ws.color, self.tab == Some(w)).interact(Sense::drag());
                if tab.clicked() {
                    actions.push(Action::Tab(Some(w)));
                }
                // (Taking a payload on release takes it whatever its type: each is looked at first.)
                if tab.dnd_hover_payload::<Vec<String>>().is_some() {
                    ui.painter().rect_stroke(tab.rect, 3.0, Stroke::new(2.0, theme::palette(ui.visuals()).user), StrokeKind::Outside);
                    if let Some(ids) = tab.dnd_release_payload::<Vec<String>>() {
                        actions.push(Action::MoveTo((*ids).clone(), w));
                    }
                }
                // Dragged among the tabs: a line on the side of this one it would go.
                if tab.drag_started() {
                    tab.dnd_set_drag_payload(TabDrag(w));
                }
                if tab.dragged()
                    && let Some(pos) = ui.ctx().pointer_interact_pos()
                {
                    let painter = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::new("tab-drag-ghost")));
                    painter.text(pos + egui::vec2(14.0, 0.0), Align2::LEFT_CENTER, workspaces::label(&ws.name), egui::FontId::proportional(14.0), theme::palette(ui.visuals()).user);
                }
                if let Some(from) = tab.dnd_hover_payload::<TabDrag>().map(|d| d.0).filter(|&from| from != w) {
                    let after = ui.ctx().pointer_interact_pos().is_some_and(|p| p.x > tab.rect.center().x);
                    let x = if after { tab.rect.right() + 2.0 } else { tab.rect.left() - 2.0 };
                    ui.painter().vline(x, tab.rect.y_range(), Stroke::new(2.0, theme::palette(ui.visuals()).user));
                    if tab.dnd_release_payload::<TabDrag>().is_some() {
                        actions.push(Action::MoveTab(from, if after { w + 1 } else { w }));
                    }
                }
                tab.context_menu(|ui| {
                    if ui.button("Rename scope").clicked() {
                        self.workspace_rename = Some((w, ws.name.clone(), true));
                    }
                    ui.menu_button("Color", |ui| {
                        if let Some(c) = color_menu(ui, ws.color.unwrap_or([128, 128, 128])) {
                            actions.push(Action::ColorWorkspace(w, c));
                        }
                    });
                    if ui.button("Delete scope").clicked() {
                        actions.push(Action::DeleteWorkspace(w));
                    }
                });
            }
            if ui.button("+").on_hover_text("New scope").clicked() {
                self.new_workspace = Some(String::new());
            }
        });
        let Some(name) = &mut self.new_workspace else { return };
        let id = Id::new("new-workspace");
        let resp = ui.add(egui::TextEdit::singleline(name).id(id).hint_text("Scope name, Enter to create"));
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        if resp.lost_focus() {
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                let name = name.clone();
                self.sync_workspaces(None);
                if let Some(w) = self.workspaces.add(&name) {
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
            ui.label(RichText::new(format!("Delete scope \"{}\"?", workspaces::label(&name))).strong());
            ui.label("Its sessions are not deleted; they stay in SCOPES.");
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

    /// "Delete from the context?" for a message's 🗑, when the setting asks for it.
    fn confirm_delete_message_modal(&mut self, ctx: &egui::Context) {
        let Some((id, i, text)) = self.confirm_delete_message.clone() else { return };
        let (mut delete, mut cancel) = (false, false);
        let modal = egui::Modal::new(Id::new("confirm-delete-message")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(RichText::new("Delete this message from the context?").strong());
            let shown = text.lines().find(|l| !l.trim().is_empty() && !l.starts_with("[tool:")).unwrap_or("(tool calls only)");
            let snippet: String = shown.chars().take(120).collect();
            ui.label(RichText::new(if snippet.len() < shown.len() { format!("{snippet}…") } else { snippet }).italics().weak());
            ui.add_space(4.0);
            ui.label("Claude won't see it from the next prompt on. A reply goes with its tool calls and their results; files it changed stay as they are.");
            let backups = settings::config_dir().join("backups");
            ui.label(RichText::new(format!("The session file as it was is kept in {}.", backups.display())).weak().small());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                delete = ui.button("Delete").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if delete {
            self.delete_message(&id, i, &text);
        }
        if delete || cancel || modal.should_close() {
            self.confirm_delete_message = None;
        }
    }

    /// "Move to Trash?" for sessions picked to delete.
    fn confirm_trash_modal(&mut self, ctx: &egui::Context) {
        if self.confirm_trash.is_empty() {
            return;
        }
        let titles: Vec<String> = self
            .confirm_trash
            .iter()
            .filter_map(|id| self.sessions.iter().position(|s| &s.id == id))
            .map(|i| self.title(i).to_owned())
            .collect();
        let (mut delete, mut cancel) = (false, false);
        let modal = egui::Modal::new(Id::new("confirm-trash")).show(ctx, |ui| {
            let heading = match titles.as_slice() {
                [one] => format!("Move \"{one}\" to the Trash?"),
                _ => format!("Move {} sessions to the Trash?", titles.len()),
            };
            ui.label(RichText::new(heading).strong());
            if titles.len() > 1 {
                for t in titles.iter().take(8) {
                    ui.label(format!("• {t}"));
                }
                if titles.len() > 8 {
                    ui.label(RichText::new(format!("and {} more", titles.len() - 8)).weak());
                }
            }
            ui.label("Their files go to the Trash; Finder's Put Back restores them.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                delete = ui.button("Move to Trash").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if delete {
            let ids = std::mem::take(&mut self.confirm_trash);
            self.trash_sessions(&ids);
        }
        if cancel || modal.should_close() {
            self.confirm_trash.clear();
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
                    ui.label("Dictation is ready: click the mic (or Cmd+D) to start, click again or press Enter to stop.");
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
                    undo = tip(ui.link("Undo refine"), "Bring back the text you had before refining", None, &format!("{cmd}+Z")).clicked();
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
        let label = if self.prompt_collapsed { "Expand the input" } else { "Collapse the input to one line" };
        toggle.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        let toggle = toggle.on_hover_text(label);
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
        // While recording, Enter stops it like Cmd+D (it doesn't send), from the input or with
        // nothing focused (clicking the mic takes focus away). Then the prompt has focus again.
        let nothing_focused = ui.memory(|m| m.focused().is_none());
        if matches!(self.dictation, Dictation::Recording(_))
            && (focused || nothing_focused)
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
        {
            self.toggle_dictation();
            self.focus_prompt = true;
        }
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
        // Cmd+D dictates (starts, or stops and transcribes) and Cmd+R refines, wherever the
        // focus is; the input has it afterwards.
        let (dictate_key, refine_anywhere) = ui.input_mut(|i| {
            (dictation::AVAILABLE && i.consume_key(Modifiers::COMMAND, Key::D), i.consume_key(Modifiers::COMMAND, Key::R))
        });
        let recording_now = matches!(self.dictation, Dictation::Recording(_));
        let dictate_key = dictate_key && (recording_now || enabled);
        let refine_key = refine_key || (refine_anywhere && !self.input.trim().is_empty());
        if dictate_key || refine_anywhere {
            self.focus_prompt = true;
        }
        // Cmd+V with images on the clipboard: they go in as markers, instead of any text that
        // came with them (a copied file's name).
        if images::paste_pressed()
            && focused
            && enabled
            && let Some(added) = images::from_clipboard()
        {
            ui.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
            self.insert_images(added);
        }
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
                // The prompt's images, in order: hover for a bigger view, click to open, × to
                // take one out (its marker in the text goes too).
                let names: Vec<String> = images::names(&self.input).into_iter().map(String::from).collect();
                if !names.is_empty() {
                    let mut remove = None;
                    ui.horizontal_wrapped(|ui| {
                        for (n, name) in names.iter().enumerate() {
                            ui.push_id(n, |ui| {
                                let r = image_thumb(ui, name, 64.0);
                                let x = egui::Rect::from_min_size(egui::pos2(r.rect.right() - 16.0, r.rect.top()), egui::vec2(16.0, 16.0));
                                let close = egui::Button::new(RichText::new("×").small().color(Color32::WHITE)).fill(Color32::from_black_alpha(160)).corner_radius(8.0);
                                if ui.put(x, close).on_hover_text("Remove this image").clicked() {
                                    remove = Some(n);
                                }
                            });
                        }
                    });
                    if let Some(n) = remove {
                        self.input = images::remove(&self.input, n);
                        self.focus_prompt = true;
                    }
                    ui.add_space(4.0);
                }
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(">").monospace().strong().color(pal.prompt_sign));
                    let send_key = if self.settings.send_with_modifier { "cmd+enter" } else { "enter" };
                    let hint = match (&self.open, running) {
                        _ if recording => "listening… enter, cmd+d or ⏹ stops and transcribes".to_owned(),
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
                            // Image markers show as chips in the text.
                            let font = egui::TextStyle::Monospace.resolve(ui.style());
                            let (text_color, chip) = (pal.prompt_text, pal.user.gamma_multiply(0.3));
                            let mut chips = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                                ui.fonts_mut(|f| f.layout_job(images::layout(buf.as_str(), font.clone(), text_color, chip, wrap)))
                            };
                            let has_images = images::has_images(&self.input);
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
                            let edit = if has_images { edit.layouter(&mut chips) } else { edit };
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
                        let resp = ui.add_enabled(enabled && !waiting && !self.input.trim().is_empty(), refine);
                        let why = if waiting { "Refining…" } else if enabled { "Type a prompt first" } else { "Select a session first" };
                        refine_click = tip(
                            resp,
                            "Refine: a helper model rewrites the prompt clearly, asking questions if it is ambiguous",
                            Some(why),
                            &format!("{cmd}+R, or {keys} in the input"),
                        )
                        .clicked();
                        // Added after refine, so it sits to the left of it.
                        if dictation::AVAILABLE {
                            let transcribing = matches!(self.dictation, Dictation::Transcribing(_));
                            let (icon, color) = if recording { ("⏹", pal.error) } else { ("🎤", ui.visuals().text_color()) };
                            let mic = egui::Button::new(RichText::new(icon).color(color)).frame(false);
                            let what = if recording {
                                "Stop dictating: what you said is transcribed into the prompt"
                            } else {
                                "Dictate: speak, and the words go into the prompt at the cursor (transcribed on this Mac)"
                            };
                            let resp = ui.add_enabled(recording || (enabled && !transcribing), mic);
                            let why = if transcribing { "Transcribing…" } else { "Select a session first" };
                            let keys = if recording { format!("Enter or {cmd}+D") } else { format!("{cmd}+D to start, again (or Enter) to stop") };
                            dictate_click = tip(resp, what, Some(why), &keys).clicked();
                            if transcribing {
                                ui.spinner();
                                ui.label(RichText::new("transcribing…").small().weak());
                            }
                            if let Dictation::Recording(recorder) = &self.dictation {
                                recording_meter(ui, recorder, &mut self.mic_levels, pal.error);
                            }
                        }
                        if running {
                            stop = tip(ui.button("⏹"), "Stop Claude: its run ends now, and your prompt comes back into the input", None, "Ctrl+C").clicked();
                            ui.spinner();
                        }
                    });
                });
            });
        if stop {
            self.stop_open();
        }
        if dictate_click || dictate_key {
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
                "This scope is empty"
            } else {
                "Select a session on the left"
            }));
            return;
        };
        if let Some(dir) = self.new_session.clone().filter(|_| s.messages.is_empty()) {
            if self.new_info.as_ref().is_none_or(|(d, _)| *d != dir) {
                let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
                let extra: Vec<String> = self.settings.extra_args.split_whitespace().map(str::to_owned).collect();
                let scope = crate::newinfo::Scope { model: &self.settings.model, extra: &extra };
                let info = crate::newinfo::gather(&dir, &home, &scope, &|k| std::env::var(k).ok());
                self.new_info = Some((dir.clone(), info));
            }
            let running = self.open_running();
            let status = if running { "Starting…" } else { "Type your first prompt below." };
            if let Some((_, info)) = &self.new_info {
                crate::newinfo::show(ui, &dir, info, &mut self.new_choice, !running, status);
            }
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
        let mut delete = None;
        // Deleting rewrites the session file: not while a run writes to it, nor before it exists.
        let deletable = !self.open_running() && self.new_session.is_none() && self.selected.is_some();
        let open_id = self.open_id();
        let key = self.view_key();
        let mut find = self.find.open.then_some(&mut self.find);
        // With the keyboard (and no field focused), Up/Down move the pick a message at a time
        // and bring its start to the top; Esc or typing goes back to the input.
        if self.conversation_keys && ui.memory(|m| m.focused().is_none()) {
            let (up, down, esc, typed) = ui.input_mut(|i| {
                let typed: String = i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.as_str()) } else { None }).collect();
                (i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::ArrowDown), i.key_pressed(Key::Escape), typed)
            });
            if (up || down)
                && let Some(next) = next_pick(&s.messages, self.picked_message, self.top_message, up)
            {
                self.picked_message = Some(next);
                self.scroll_to_pick = true;
            }
            if esc || !typed.is_empty() {
                self.conversation_keys = false;
                self.focus_prompt = true;
                if !typed.is_empty() {
                    self.input.push_str(&typed);
                    let id = Id::new("prompt-input");
                    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(self.input.chars().count()))));
                    state.store(ui.ctx(), id);
                }
            }
        }
        let click = ui.input(|i| i.pointer.primary_clicked().then_some(i.pointer.interact_pos()).flatten());
        let pick_stroke = Stroke::new(1.5, ui.visuals().selection.bg_fill);
        let mut top_message = None;
        let out = egui::ScrollArea::vertical().id_salt(("conversation", &key)).auto_shrink(false).stick_to_bottom(true).show_viewport(ui, |ui, viewport| {
            let origin = ui.cursor().top();
            // Render only messages near the viewport; the rest just take their remembered height.
            let near = viewport.expand2(egui::vec2(0.0, 400.0));
            for (i, m) in s.messages.iter().enumerate() {
                let y = ui.cursor().top() - origin;
                if pinned_text(m).is_some() && y <= viewport.min.y + 1.0 {
                    top_prompt = Some(i);
                }
                if pickable(m) && y <= viewport.min.y + 1.0 {
                    top_message = Some(i);
                }
                let h = self.heights[i];
                let skip = h > 0.0 && (y + h < near.min.y || y > near.max.y);
                let from = find::next_shape(ui);
                let rect = if skip {
                    ui.allocate_space(egui::vec2(ui.available_width(), h)).1
                } else {
                    let shown = ui.push_id(i, |ui| message(ui, &mut self.md_cache, m, &pal, Some(deletable)));
                    let (card, delete_clicked) = shown.inner;
                    if delete_clicked {
                        delete = Some(i);
                    }
                    let rect = shown.response.rect;
                    self.heights[i] = rect.height().max(1.0);
                    // A click on a card picks it (a drag still selects text in it).
                    if let Some(card) = card {
                        if click.is_some_and(|p| card.contains(p) && ui.clip_rect().contains(p)) {
                            self.picked_message = Some(i);
                        }
                        if self.conversation_keys && self.picked_message == Some(i) {
                            ui.painter().rect_stroke(card, 6.0, pick_stroke, StrokeKind::Outside);
                        }
                    }
                    rect
                };
                if self.scroll_to_pick && self.picked_message == Some(i) {
                    ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                    self.scroll_to_pick = false;
                }
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
                    ui.push_id("live", |ui| message(ui, &mut self.md_cache, &m, &pal, None));
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
                    ui.push_id("interrupted", |ui| message(ui, &mut self.md_cache, &m, &pal, None));
                }
                ui.label(RichText::new("⏹ Interrupted").color(pal.note).monospace());
            }
        });
        self.conversation_rect = Some(out.inner_rect);
        self.top_message = top_message;
        if top_prompt != self.pinned || inline_you != self.inline_you {
            self.pinned = top_prompt;
            self.inline_you = inline_you;
            ui.ctx().request_repaint();
        }
        if let Some(i) = delete {
            self.ask_delete_message(i);
        }
    }

    /// 🗑 on message `i` of the open session: asks first when the setting says so.
    fn ask_delete_message(&mut self, i: usize) {
        let Some(text) = self.open.as_ref().and_then(|s| s.messages.get(i)).map(|m| m.text.clone()) else { return };
        let Some(id) = self.open_id() else { return };
        if self.settings.confirm_delete_message {
            self.confirm_delete_message = Some((id, i, text));
        } else {
            self.delete_message(&id, i, &text);
        }
    }

    /// Takes message `i` (saying `text`) of session `id` out of its file, so Claude no longer
    /// sees it (see `session::delete_message`); the file as it was goes to the config folder's
    /// `backups`. The open session then shows what is left.
    fn delete_message(&mut self, id: &str, i: usize, text: &str) {
        if self.runs.contains_key(id) {
            self.error = Some("Claude is working in this session; delete the message once it has finished".into());
            return;
        }
        let Some(path) = self.sessions.iter().find(|s| s.id == id).map(|s| s.path.clone()) else { return };
        let stamp = SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let backup = settings::config_dir().join("backups").join(format!("{id}-{stamp}.jsonl"));
        match session::delete_message(&path, i, text, &backup) {
            Ok(()) if self.open_id().as_deref() == Some(id) => self.reload(),
            Ok(()) => {}
            Err(e) => self.error = Some(format!("could not delete the message: {e}")),
        }
    }

    /// Pinned above a file tab, where the conversation pins "You": the path with a copy button,
    /// and what's worth knowing about the file (on disk and in this session).
    fn file_header(&self, ui: &mut egui::Ui, i: usize) {
        let pal = theme::palette(ui.visuals());
        let tab = &self.file_tabs[i];
        let mut meta = vec![diffview::language_name(&tab.path)];
        if let Ok(text) = &tab.contents {
            let n = text.lines().count();
            meta.push(format!("{n} line{}", if n == 1 { "" } else { "s" }));
        }
        match std::fs::metadata(&tab.path) {
            Ok(m) => {
                meta.push(file_size(m.len()));
                if let Some(age) = m.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()) {
                    meta.push(format!("modified {} ago", short_duration(age)));
                }
                if m.permissions().readonly() {
                    meta.push("read-only".to_owned());
                }
            }
            Err(_) => meta.push("not on disk".to_owned()),
        }
        if let Some(s) = &self.open {
            if let Some(rel) = s.cwd.as_ref().and_then(|cwd| std::path::Path::new(&tab.path).strip_prefix(cwd).ok()) {
                meta.push(format!("./{}", rel.display()));
            }
            match diffview::stats(&s.changes, &tab.path) {
                (0, ..) => meta.push("not changed in this session".to_owned()),
                (n, added, removed) => meta.push(format!("+{added} -{removed} in {n} edit{} this session", if n == 1 { "" } else { "s" })),
            }
        }
        let copied_id = Id::new(("copied-path", &tab.path));
        let now = ui.input(|i| i.time);
        let copied = ui.data(|d| d.get_temp::<f64>(copied_id)).is_some_and(|t| now - t < 1.5);
        let frame = egui::Frame::new().fill(ui.visuals().faint_bg_color).stroke(Stroke::new(1.0, pal.border));
        frame.inner_margin(6.0).corner_radius(4.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let button_width = 64.0;
                ui.label(RichText::new("📄 File").strong().color(pal.user));
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - button_width).max(0.0));
                    ui.add(egui::Label::new(RichText::new(&tab.path).strong().monospace().color(pal.user)).truncate());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if copied { "✔ Copied" } else { "📋 Copy" };
                    if ui.small_button(label).on_hover_text("Copy the full path").clicked() {
                        ui.ctx().copy_text(tab.path.clone());
                        ui.data_mut(|d| d.insert_temp(copied_id, now));
                    }
                });
            });
            ui.label(RichText::new(meta.join(" · ")).weak().small());
        });
        if copied {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        // A rule between the header and the contents, so the path doesn't read as the file's first line.
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);
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
        self.file_header(ui, i);
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
        let mut about = settings::About { status_line: &status_line, updater: &mut self.updater };
        if settings::ui(ui, &mut self.settings, &mut self.settings_filter, &mut about) {
            self.apply_settings();
            if let Err(e) = self.settings.save() {
                self.error = Some(format!("could not save settings: {e}"));
            }
        }
    }

    /// Zoom is changed with Cmd +/- (no slider); keep the one in use so the next launch has it.
    /// Only a change made here is saved: a zoom another scope saved takes a frame to apply,
    /// and saving the old one meanwhile would undo it.
    fn keep_zoom(&mut self, ctx: &egui::Context) {
        let zoom = ctx.zoom_factor();
        if (zoom - self.zoom_seen).abs() < 0.001 {
            return;
        }
        self.zoom_seen = zoom;
        if (zoom - self.settings.zoom).abs() < 0.001 {
            return;
        }
        self.settings.zoom = zoom;
        if let Err(e) = self.settings.save() {
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

    /// The model a run in `dir` gets when its session picked none (Settings, extra arguments,
    /// `ANTHROPIC_MODEL`, then Claude Code's settings files); `None` = Claude Code's default.
    fn default_model(&mut self, dir: &std::path::Path) -> Option<crate::newinfo::Sourced> {
        let key = (dir.to_path_buf(), self.settings.model.clone(), self.settings.extra_args.clone());
        if let Some((k, at, m)) = &self.default_model
            && *k == key
            && at.elapsed() < std::time::Duration::from_secs(5)
        {
            return m.clone();
        }
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let extra: Vec<String> = key.2.split_whitespace().map(str::to_owned).collect();
        let scope = crate::newinfo::Scope { model: &key.1, extra: &extra };
        let m = crate::newinfo::default_model(dir, &home, &scope, &|k| std::env::var(k).ok());
        self.default_model = Some((key, std::time::Instant::now(), m.clone()));
        m
    }

    /// The model picked for the open session (empty = the default): the new session page's
    /// pick until its first prompt, then the session's own.
    fn open_model_choice(&self) -> String {
        match self.open_id() {
            Some(id) => self.choices.get(&id).map(|c| c.model.clone()).unwrap_or_default(),
            None => self.new_choice.model.clone(),
        }
    }

    /// Picks `model` for the open session (empty = the default); its next runs use it.
    fn set_open_model(&mut self, model: String) {
        let Some(id) = self.open_id() else {
            self.new_choice.model = model;
            return;
        };
        let c = self.choices.entry(id.clone()).or_default();
        c.model = model;
        if c.is_empty() {
            self.choices.remove(&id);
        }
        self.save_state();
    }

    fn info(&mut self, ui: &mut egui::Ui) {
        let dir = self.new_session.clone().or_else(|| self.open.as_ref().and_then(|s| s.cwd.clone()));
        let default_model = dir.map(|d| self.default_model(&d));
        let mut model_pick = self.open_model_choice();
        let picked_before = model_pick.clone();
        let model_editable = !self.open_running();
        let Some(s) = &self.open else {
            ui.label("Select a session to see its info");
            return;
        };
        let id = self.selected.map(|i| self.sessions[i].id.clone());
        let mut start_summary = false;
        let mut toggled = None;
        let (mut ask, mut close) = (false, None);
        // Questions about the goal sit at the very bottom, answered with the session's context
        // but not added to it; the answers stack above the field. Text wraps to the panel's
        // width, so field and answers grow upwards until they meet the sections, then scroll.
        if let Some(id) = id.as_ref().filter(|_| !s.messages.is_empty()) {
            let room = ui.available_height();
            let max_total = (room - self.info_height - 12.0).max(room * 0.3).max(120.0);
            let pal = theme::palette(ui.visuals());
            let field_id = Id::new("goal-question-input");
            // Enter sends, Shift+Enter adds a line: taken before the TextEdit sees it.
            if ui.memory(|m| m.has_focus(field_id)) && !self.goal_question.trim().is_empty() {
                ask = ui.input_mut(|i| {
                    let send = i.key_pressed(Key::Enter) && !i.modifiers.shift;
                    if send {
                        i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::Enter, .. }));
                    }
                    send
                });
            }
            let list = self.btw.get(id).map(Vec::as_slice).unwrap_or_default();
            egui::Panel::bottom("goal-question")
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
                .show_separator_line(false)
                .resizable(false)
                .show(ui, |ui| {
                    if !list.is_empty() {
                        let max_height = (max_total - self.goal_field_height - 16.0).max(60.0);
                        egui::ScrollArea::vertical()
                            .id_salt("goal-answers")
                            .auto_shrink([false, true])
                            .max_height(max_height)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for (i, b) in list.iter().enumerate() {
                                    ui.push_id(i, |ui| {
                                        egui::Frame::new()
                                            .fill(ui.visuals().faint_bg_color)
                                            .stroke(Stroke::new(1.0, pal.note))
                                            .inner_margin(8.0)
                                            .corner_radius(4.0)
                                            .show(ui, |ui| {
                                                ui.set_width(ui.available_width());
                                                ui.horizontal_top(|ui| {
                                                    let text = (ui.available_width() - 24.0).max(0.0);
                                                    ui.allocate_ui_with_layout(egui::vec2(text, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                                        ui.add(egui::Label::new(RichText::new(&b.question).italics()).wrap());
                                                    });
                                                    if ui.small_button("×").on_hover_text("Close").clicked() {
                                                        close = Some(i);
                                                    }
                                                });
                                                match &b.answer {
                                                    None => {
                                                        ui.horizontal(|ui| {
                                                            ui.spinner();
                                                            ui.label("Thinking (not added to the session)…");
                                                        });
                                                    }
                                                    Some(Ok(a)) => markdown::show(ui, &mut self.md_cache, a),
                                                    Some(Err(e)) => {
                                                        ui.add(egui::Label::new(RichText::new(e).color(pal.error)).wrap());
                                                    }
                                                }
                                            });
                                    });
                                    ui.add_space(4.0);
                                }
                            });
                    }
                    let field_max = if list.is_empty() { max_total } else { (max_total - 80.0).max(40.0) };
                    let out = egui::Frame::new()
                        .fill(ui.visuals().extreme_bg_color)
                        .stroke(ui.visuals().widgets.inactive.bg_stroke)
                        .inner_margin(4.0)
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            let width = ui.available_width();
                            egui::ScrollArea::vertical()
                                .id_salt("goal-question-scroll")
                                .auto_shrink([false, true])
                                .max_height(field_max)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    let field = egui::TextEdit::multiline(&mut self.goal_question)
                                        .id(field_id)
                                        .frame(egui::Frame::NONE)
                                        .hint_text("Ask about this goal, Enter to send")
                                        .desired_rows(1)
                                        .return_key(Some(egui::KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)))
                                        .desired_width(width);
                                    ui.add(field);
                                })
                        })
                        .response;
                    if (out.rect.height() - self.goal_field_height).abs() > 0.5 {
                        self.goal_field_height = out.rect.height();
                        ui.ctx().request_repaint();
                    }
                });
        }
        let sections = egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
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
            // What the next prompt runs with: the session's own pick, else the default. Runs
            // resume the same session, so switching keeps the conversation.
            let (default, from) = match default_model.flatten() {
                Some(m) => (m.value, m.from),
                None => ("default".to_owned(), "Claude Code's default".to_owned()),
            };
            let options: Vec<(String, String)> = crate::newinfo::MODELS.iter().map(|m| ((*m).to_owned(), (*m).to_owned())).collect();
            ui.horizontal_wrapped(|ui| {
                crate::newinfo::picker(ui, "info-model", &mut model_pick, &options, &format!("{default} (as set)"), model_editable);
                let note = if model_pick.is_empty() { format!("({from})") } else { "(this session; later prompts keep it)".to_owned() };
                ui.label(RichText::new(note).weak().small());
            });
            if !model_editable {
                ui.label(RichText::new("Can be changed once this run ends").weak().small());
            }
            if let Some(m) = &s.model {
                ui.label(RichText::new(format!("Latest reply: {m}")).weak().small().monospace());
            }

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

            // The 5-hour session limit gets its own bar under the context; the weekly ones below.
            section(ui, "Session limit");
            match known_limits(self.usage.as_ref()).and_then(|(w, at)| Some((w.into_iter().find(|w| w.short == "5h")?, at))) {
                Some((w, at)) => limit_bars(ui, std::slice::from_ref(&w), at, false),
                None => {
                    ui.label(RichText::new("Not known yet (see Usage limits)").weak());
                }
            }

            section(ui, "Usage limits");
            if limits_ui(ui, self.usage.as_ref(), self.usage_error.as_deref()) {
                match status::enable() {
                    Ok(()) => self.error = None,
                    Err(e) => self.error = Some(format!("could not enable status line: {e}")),
                }
            }
        });
        if (sections.content_size.y - self.info_height).abs() > 0.5 {
            self.info_height = sections.content_size.y;
            ui.ctx().request_repaint();
        }
        if let (Some(i), Some(id)) = (toggled, &id) {
            if let Err(e) = self.goals.change(|g| g.set(id, &goal::toggle(g.get(id).unwrap_or_default(), i))) {
                self.error = Some(format!("could not save goal: {e}"));
            }
        }
        if let (Some(i), Some(id)) = (close, &id)
            && let Some(list) = self.btw.get_mut(id)
        {
            list.remove(i);
            if list.is_empty() {
                self.btw.remove(id);
            }
        }
        if let (true, Some(id)) = (ask, &id) {
            let question = std::mem::take(&mut self.goal_question).trim().to_owned();
            let harness = Settings::harness(&self.settings.question_harness, goal::QUESTION_HARNESS);
            let rx = runner::btw(ui.ctx().clone(), id.clone(), s.cwd.clone(), goal::question_prompt(harness, self.goals.get(id), &question));
            self.btw.entry(id.clone()).or_default().push(Btw { question, answer: None, rx: Some(rx) });
        }
        if let (true, Some(id)) = (start_summary, id) {
            let rx = runner::summarize_goal(ui.ctx().clone(), session::digest(&s.messages, 400, 12_000));
            self.summarizing = Some((id, rx));
        }
        if model_pick != picked_before {
            self.set_open_model(model_pick);
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
                    let folder = diffview::folder_in(&f.file, s.cwd.as_deref());
                    let selected = self.changes_filter.as_deref() == Some(f.file.as_str());
                    let row = ui
                        .horizontal(|ui| {
                            let r = ui.selectable_label(selected, RichText::new(name).monospace());
                            ui.label(RichText::new(format!("+{} -{}", f.added, f.removed)).weak().small());
                            r
                        })
                        .inner
                        .on_hover_text(format!("{dir}\nClick: show its changes · Double-click: open it"));
                    ui.add(egui::Label::new(RichText::new(folder).weak().small()).truncate());
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
    /// the window, double-click to zoom. The Settings ⚙ sits at its right end, dimmed until
    /// hovered; it is added after the drag area so it takes its own clicks.
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
        let size = egui::vec2(TITLE_BAR_HEIGHT, rect.height());
        let gear_rect = egui::Rect::from_min_size(egui::pos2(rect.right() - size.x - 4.0, rect.top()), size);
        let lit = ui.rect_contains_pointer(gear_rect) || (self.settings_tab && self.settings_active);
        let color = if lit { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
        let gear = egui::Button::new(RichText::new("⚙").size(15.0).color(color)).frame(false);
        let cmd = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
        if ui.put(gear_rect, gear).on_hover_text(format!("Settings ({cmd}+,)")).clicked() {
            self.open_settings();
        }
        // A newer scope: a quiet note left of the ⚙, opening Settings → About to install it.
        if let Some(version) = self.updater.available().map(|r| r.version.clone()) {
            let note_rect = egui::Rect::from_min_max(rect.left_top(), egui::pos2(gear_rect.left() - 4.0, rect.bottom()));
            let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(note_rect).layout(egui::Layout::right_to_left(egui::Align::Center)));
            let note = egui::Button::new(RichText::new(format!("scope {version} is available")).small()).frame(false);
            if bar.add(note).on_hover_text("Open Settings to install it").clicked() {
                self.settings_filter.clear();
                self.open_settings();
            }
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
        // Cmd+Z with no field focused (a focused one undoes its own typing) brings back the
        // sessions taken out of a workspace last.
        let modal = !self.confirm_trash.is_empty() || self.confirm_delete.is_some();
        if !self.removed.is_empty()
            && !modal
            && ctx.memory(|m| m.focused().is_none())
            && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z))
        {
            self.undo_remove();
        }
        if !self.picked.is_empty() && self.confirm_trash.is_empty() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.picked.clear();
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
/// so scope's own copy of the partial reply isn't shown twice.
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
/// The session to open once `removed` leave the bar listing `order`: the one just above the
/// topmost of them, or, when that was the first, the one now on top.
fn after_remove(order: &[String], removed: &[String]) -> Option<String> {
    let first = order.iter().position(|id| removed.contains(id))?;
    let above = first.checked_sub(1).map(|k| &order[k]);
    above.or_else(|| order.iter().find(|id| !removed.contains(id))).cloned()
}

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
    tab.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, galley.text()));
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
fn pending_new(runs: &HashMap<String, Run>, ended: &[(String, String, Option<PathBuf>)], listed: &[SessionMeta], members: Option<&[String]>) -> Vec<(String, String, String)> {
    let running = runs.iter().filter(|(_, r)| r.new).map(|(id, r)| (id, &r.prompt, &r.cwd));
    let mut out: Vec<_> = running
        .chain(ended.iter().map(|(id, prompt, cwd)| (id, prompt, cwd)))
        .filter(|(id, ..)| !listed.iter().any(|s| &s.id == *id))
        .filter(|(id, ..)| members.is_none_or(|m| m.contains(id)))
        .map(|(id, prompt, cwd)| {
            let folder = cwd.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
            (id.clone(), prompt.lines().next().unwrap_or_default().chars().take(60).collect(), folder)
        })
        .collect();
    out.dedup_by(|a, b| a.0 == b.0);
    out.sort();
    out
}

/// "/Users/me/Desktop/proj" → "Desktop/proj"
/// A session in the list: its name in a rounded bubble, with a spinner on the right while
/// Claude is working in it, and a search note under the name when there is one. Senses click
/// and drag in one widget (a separate drag-source wrapper would swallow the click).
/// `tag`: how its last run ended, shown before the time (its reason on hover).
#[allow(clippy::too_many_arguments)]
fn session_bubble(ui: &mut egui::Ui, selected: bool, title: &str, note: Option<&str>, when: Option<&str>, running: bool, color: Option<[u8; 3]>, tag: Option<&Verdict>) -> egui::Response {
    const PAD: egui::Vec2 = egui::vec2(10.0, 6.0);
    const SPIN: f32 = 12.0;
    let width = ui.available_width();
    let text_width = (width - 2.0 * PAD.x - if running { SPIN + 6.0 } else { 0.0 }).max(10.0);
    let mut job = egui::text::LayoutJob::simple(title.to_owned(), egui::TextStyle::Body.resolve(ui.style()), ui.visuals().text_color(), text_width);
    job.wrap.max_rows = 2;
    let galley = ui.painter().layout_job(job);
    let note = note.map(|n| ui.painter().layout(n.to_owned(), egui::TextStyle::Small.resolve(ui.style()), ui.visuals().weak_text_color(), text_width));
    // The time, small at the bottom right: beside the last line when it fits, else below it.
    let pal = theme::palette(ui.visuals());
    let small = egui::FontId::proportional(10.0);
    let tag_color = |v: &Verdict| match v.state.as_str() {
        "done" => pal.add_fg,
        "blocked" => pal.error,
        _ => pal.note,
    };
    let when = match (when, tag) {
        (None, None) => None,
        (w, t) => {
            let mut job = egui::text::LayoutJob::default();
            if let Some(v) = t {
                // Only the icon takes the state's color; its word reads as ordinary text.
                let (icon, word) = v.label().split_once(' ').unwrap_or((v.label(), ""));
                job.append(icon, 0.0, egui::TextFormat::simple(small.clone(), tag_color(v)));
                job.append(word, 3.0, egui::TextFormat::simple(small.clone(), ui.visuals().text_color()));
            }
            if let Some(w) = w {
                job.append(w, if t.is_some() { 6.0 } else { 0.0 }, egui::TextFormat::simple(small.clone(), ui.visuals().weak_text_color()));
            }
            Some(ui.painter().layout_job(job))
        }
    };
    let last_line = note.as_ref().unwrap_or(&galley).rows.last().map_or(0.0, |r| r.rect().width());
    let below = when.as_ref().filter(|w| last_line + 8.0 + w.size().x > text_width).map_or(0.0, |w| w.size().y);
    let height = galley.size().y + note.as_ref().map_or(0.0, |n| n.size().y + 2.0) + below + 2.0 * PAD.y;
    let (rect, row) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click_and_drag());
    let mut label = match &note {
        Some(n) => format!("{title}\n{}", n.text()),
        None => title.to_owned(),
    };
    if let Some(v) = tag {
        label.push_str(&format!("\n{}", v.label()));
    }
    row.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &label));
    if ui.is_rect_visible(rect) {
        let v = ui.visuals();
        let fill = if selected {
            v.selection.bg_fill
        } else if row.hovered() {
            v.widgets.hovered.weak_bg_fill
        } else {
            v.faint_bg_color
        };
        let stroke = if selected { v.selection.stroke } else { v.widgets.noninteractive.bg_stroke };
        let radius = 10.0;
        ui.painter().rect(rect, radius, fill, stroke, StrokeKind::Inside);
        // The workspace a session belongs to shows only as a bar down its left edge.
        if let Some(c) = color {
            let bar = egui::Rect::from_min_max(egui::pos2(rect.left() + 3.0, rect.top() + 5.0), egui::pos2(rect.left() + 6.0, rect.bottom() - 5.0));
            ui.painter().rect_filled(bar, 1.5, rgb(c));
        }
        let text_color = if selected { v.selection.stroke.color } else { v.text_color() };
        let mut pos = rect.min + PAD;
        let galley_height = galley.size().y;
        ui.painter().galley(pos, galley, text_color);
        if let Some(n) = note {
            pos.y += galley_height + 2.0;
            ui.painter().galley(pos, n, v.weak_text_color());
        }
        if let Some(w) = when {
            let at = egui::pos2(rect.min.x + PAD.x + text_width, rect.max.y - PAD.y);
            ui.painter().galley(at - w.size(), w, v.weak_text_color());
        }
        if running {
            let spin = egui::Rect::from_center_size(egui::pos2(rect.max.x - PAD.x - SPIN / 2.0, rect.center().y), egui::vec2(SPIN, SPIN));
            egui::Spinner::new().size(SPIN).paint_at(ui, spin);
        }
    }
    ui.add_space(2.0);
    match tag {
        _ if running => row.on_hover_text_at_pointer("Claude is working in this session"),
        Some(v) if !v.why.is_empty() => row.on_hover_text_at_pointer(&v.why),
        _ => row,
    }
}

/// One message as a card: role label and accent stripe, then its rendered Markdown.
/// Messages that are only tool calls show as a single dimmed line. With `delete` (whether
/// deleting is possible now), the label's row also has Delete. Returns where the card shows
/// (what a click picks; None for what shows no card: a lone stop, tool lines) and whether
/// Delete was clicked.
fn message(ui: &mut egui::Ui, cache: &mut CommonMarkCache, m: &session::Message, pal: &theme::Palette, delete: Option<bool>) -> (Option<egui::Rect>, bool) {
    if m.role == Role::User && is_interrupt_marker(&m.text) {
        // Claude's record of a stop: alone, after the prompt it stopped, or (merged by the
        // parser) before the next prompt. Each part shows in its place; Delete takes the whole.
        let (before, after) = split_interrupt_marker(&m.text);
        let mut card = None;
        let mut clicked = false;
        if !before.is_empty() {
            (card, clicked) = ui.push_id("before", |ui| message(ui, cache, &session::Message { role: Role::User, text: before }, pal, delete)).inner;
        }
        ui.label(RichText::new("⏹ Interrupted").color(pal.note).monospace());
        ui.add_space(6.0);
        if !after.is_empty() {
            let (r, c) = ui.push_id("after", |ui| message(ui, cache, &session::Message { role: Role::User, text: after }, pal, delete)).inner;
            clicked |= c;
            card = match (card, r) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            };
        }
        return (card, clicked);
    }
    if is_tool_lines(m) {
        if markdown::SHOW_TOOLS.load(std::sync::atomic::Ordering::Relaxed) {
            egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 0, top: 0, bottom: 6 }).show(ui, |ui| markdown::show(ui, cache, &m.text));
        }
        return (None, false);
    }
    let (who, accent) = match m.role {
        Role::User => ("You", pal.user),
        Role::Assistant => ("Claude", pal.assistant),
    };
    let fill = match m.role {
        Role::User => accent.gamma_multiply(0.08),
        Role::Assistant => ui.visuals().faint_bg_color,
    };
    let mut clicked = false;
    let card = egui::Frame::new()
        .fill(fill)
        .corner_radius(6.0)
        .inner_margin(egui::Margin { left: 14, right: 10, top: 6, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(who).strong().color(accent));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    copy_button(ui, m, fill);
                    if let Some(can_delete) = delete {
                        clicked = delete_button(ui, can_delete);
                    }
                });
            });
            if m.role == Role::User && images::has_images(&m.text) {
                with_images(ui, cache, &m.text);
            } else {
                markdown::show(ui, cache, &m.text);
            }
        });
    let r = card.response.rect;
    let stripe = egui::Rect::from_min_max(r.min, egui::pos2(r.left() + 3.0, r.bottom()));
    ui.painter().rect_filled(stripe, egui::CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, accent);
    ui.add_space(6.0);
    (Some(r), clicked)
}

/// A Claude message that is only tool calls (shown as dimmed lines, if at all).
fn is_tool_lines(m: &session::Message) -> bool {
    m.role == Role::Assistant && m.text.lines().all(|l| l.starts_with("[tool:"))
}

/// Whether the arrows stop at a message: it shows as a card (see [`message`]); tool lines and
/// a lone stop don't.
fn pickable(m: &session::Message) -> bool {
    if is_tool_lines(m) {
        return false;
    }
    let (before, after) = split_interrupt_marker(&m.text);
    m.role == Role::Assistant || !before.is_empty() || !after.is_empty()
}

/// The pickable message an arrow moves to from `picked` (or, without a pick, from the one at
/// the top): the one before it (`up`) or after it. None at either end.
fn next_pick(messages: &[session::Message], picked: Option<usize>, top: Option<usize>, up: bool) -> Option<usize> {
    let ok = |&i: &usize| messages.get(i).is_some_and(pickable);
    match (picked, top) {
        (Some(p), _) if up => (0..p.min(messages.len())).rev().find(ok),
        (Some(p), _) => (p + 1..messages.len()).find(ok),
        // Without a pick, Up brings the start of the message at the top into view first.
        (None, Some(t)) if up => (0..=t.min(messages.len().saturating_sub(1))).rev().find(ok),
        (None, Some(t)) => (t + 1..messages.len()).find(ok),
        (None, None) => (0..messages.len()).rev().find(ok),
    }
}

/// Copy at the right end of a message's label row: two overlapping rounded squares, drawn
/// (no font has them), then a check mark for a moment after copying. `fill` is the card's
/// background, which the front square is filled with so it covers the back one.
fn copy_button(ui: &mut egui::Ui, m: &session::Message, fill: Color32) {
    let copied_id = ui.id().with("copied");
    let now = ui.input(|i| i.time);
    let copied = ui.data(|d| d.get_temp::<f64>(copied_id)).is_some_and(|t| now - t < 1.5);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 18.0), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Copy"));
    let resp = resp.on_hover_text("Copy the message").on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() {
        ui.ctx().copy_text(copy_text(m));
        ui.data_mut(|d| d.insert_temp(copied_id, now));
    }
    let v = ui.visuals();
    let bg = if resp.hovered() { v.widgets.hovered.weak_bg_fill } else { fill };
    let color = if resp.hovered() { v.strong_text_color() } else { v.weak_text_color() };
    let stroke = Stroke::new(1.3, color);
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 4.0, bg);
    }
    let c = rect.center();
    if copied {
        let pts = vec![c + egui::vec2(-4.5, 0.0), c + egui::vec2(-1.5, 3.0), c + egui::vec2(4.5, -3.5)];
        p.add(egui::Shape::line(pts, stroke));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
    } else {
        let (side, shift) = (9.0, 3.5);
        let back = egui::Rect::from_min_size(c + egui::vec2(-side / 2.0 - shift / 2.0 + shift, -side / 2.0 - shift / 2.0), egui::vec2(side, side));
        let front = back.translate(egui::vec2(-shift, shift));
        p.rect_stroke(back, 1.5, stroke, egui::StrokeKind::Middle);
        p.rect_filled(front, 1.5, bg);
        p.rect_stroke(front, 1.5, stroke, egui::StrokeKind::Middle);
    }
}

/// Delete, left of Copy in a message's label row: small and without a frame until hovered.
/// True when clicked.
fn delete_button(ui: &mut egui::Ui, can_delete: bool) -> bool {
    let icon = egui::Button::new(RichText::new("🗑").small().weak()).small().frame_when_inactive(false);
    ui.add_enabled(can_delete, icon)
        .on_hover_text("Delete from the context: Claude won't see this message from the next prompt on")
        .on_disabled_hover_text("Claude is working in this session; delete once it has finished")
        .clicked()
}

/// What Copy on a message puts on the clipboard: its text, without Claude's tool lines.
fn copy_text(m: &session::Message) -> String {
    let lines: Vec<&str> = m.text.lines().filter(|l| m.role == Role::User || !l.starts_with("[tool:")).collect();
    lines.join("\n").trim().to_owned()
}

/// A prompt's text with its images as thumbnails, where they were in it.
fn with_images(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    let parts = images::parts(text);
    let mut i = 0;
    while i < parts.len() {
        ui.push_id(i, |ui| match parts[i] {
            images::Part::Text(t) => {
                if !t.trim().is_empty() {
                    markdown::show(ui, cache, t.trim());
                }
                i += 1;
            }
            images::Part::Image(_) => {
                ui.horizontal_wrapped(|ui| {
                    while let Some(images::Part::Image(name)) = parts.get(i) {
                        ui.push_id(i, |ui| image_thumb(ui, name, 140.0));
                        i += 1;
                    }
                });
            }
        });
    }
}

/// An image in a prompt, at most `height` tall: hover shows it bigger, a click opens it.
fn image_thumb(ui: &mut egui::Ui, name: &str, height: f32) -> egui::Response {
    let uri = images::uri(name);
    let r = ui
        .add(egui::Image::new(uri.clone()).max_size(egui::vec2(height * 2.5, height)).corner_radius(4.0).alt_text(format!("Image {name}")).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().rect_stroke(r.rect, 4.0, Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color), egui::StrokeKind::Outside);
    if r.clicked() {
        images::open(name);
    }
    r.on_hover_ui(|ui| {
        ui.add(egui::Image::new(uri).max_size(egui::vec2(520.0, 520.0)));
        ui.label(RichText::new("Click to open").small().weak());
    })
}

/// `text` with `c` put in at a character index.
fn insert_char(text: &str, at: usize, c: char) -> String {
    let byte = text.char_indices().nth(at).map_or(text.len(), |(b, _)| b);
    format!("{}{c}{}", &text[..byte], &text[byte..])
}

/// Editable name in place of a workspace tab. Enter or clicking away saves; Escape cancels.
fn workspace_rename_field(ui: &mut egui::Ui, w: usize, text: &mut String, fresh: &mut bool, actions: &mut Vec<Action>) {
    let id = Id::new("workspace-rename");
    let width = ui.fonts_mut(|f| f.layout_no_wrap(text.clone(), egui::TextStyle::Button.resolve(ui.style()), Color32::WHITE).size().x);
    let mut out = egui::TextEdit::singleline(text).id(id).desired_width(width.max(60.0) + 8.0).show(ui);
    if *fresh {
        *fresh = false;
        out.response.request_focus();
        let all = CCursorRange::two(CCursor::new(0), CCursor::new(text.chars().count()));
        out.state.cursor.set_char_range(Some(all));
        out.state.store(ui.ctx(), id);
    }
    if out.response.lost_focus() {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            actions.push(Action::RenameWorkspace(w, String::new()));
        } else {
            actions.push(Action::RenameWorkspace(w, text.clone()));
        }
    }
}

/// The palette as swatches, then a picker for any color. The color picked, if one was.
fn color_menu(ui: &mut egui::Ui, current: [u8; 3]) -> Option<[u8; 3]> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for c in workspaces::PALETTE {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::click());
            ui.painter().rect_filled(rect, 3.0, rgb(c));
            if c == current || resp.hovered() {
                ui.painter().rect_stroke(rect.expand(2.0), 4.0, Stroke::new(1.5, ui.visuals().text_color()), StrokeKind::Inside);
            }
            if resp.clicked() {
                picked = Some(c);
                ui.close();
            }
        }
    });
    ui.separator();
    let mut c = rgb(current);
    if egui::widgets::color_picker::color_picker_color32(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque) {
        picked = Some([c.r(), c.g(), c.b()]);
    }
    picked
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


/// Account usage limits: from the usage endpoint `/usage` reads, else from the latest
/// status-line payload. Returns true if "Enable" was clicked.
fn limits_ui(ui: &mut egui::Ui, usage: Option<&(Vec<status::Window>, SystemTime)>, error: Option<&str>) -> bool {
    // Keeps "resets in" and "updated ago" current, and re-reads the status-line file.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
    let weekly = |windows: &[status::Window]| windows.iter().filter(|w| w.short != "5h").cloned().collect::<Vec<_>>();
    if let Some((windows, at)) = usage {
        limit_bars(ui, &weekly(windows), *at, true);
        if let Some(e) = error {
            ui.label(RichText::new(format!("Last refresh failed: {e}")).weak().small());
        }
        return false;
    }
    if let Some(e) = error {
        ui.label(RichText::new(format!("Can't ask Claude for your usage ({e}); showing what the status line last saw.")).weak().small());
    }
    match status::setup() {
        status::Setup::Other(cmd) => {
            ui.label(RichText::new(format!("Claude Code limits are only shared with the status line, and yours is already set to `{cmd}`. scope won't replace it.")).weak());
            return false;
        }
        status::Setup::NotSet => {
            ui.label(RichText::new("Claude Code shares usage limits only with its status line. Enable scope's status line to see them here (sets statusLine in ~/.claude/settings.json).").weak());
            return ui.button("Enable").clicked();
        }
        status::Setup::Enabled => {}
    }
    let Some((v, at)) = status::load(&status::dir().join("latest.json")) else {
        ui.label(RichText::new("Enabled. Limits appear after your next reply in an interactive Claude Code session (Pro/Max plans only).").weak());
        return false;
    };
    limit_bars(ui, &weekly(&status::limits(&v)), at, true);
    false
}

/// The account's limits and when they were read: from the usage endpoint, else from the
/// status line's latest payload.
fn known_limits(usage: Option<&(Vec<status::Window>, SystemTime)>) -> Option<(Vec<status::Window>, SystemTime)> {
    if let Some((windows, at)) = usage {
        return Some((windows.clone(), *at));
    }
    if !matches!(status::setup(), status::Setup::Enabled) {
        return None;
    }
    status::load(&status::dir().join("latest.json")).map(|(v, at)| (status::limits(&v), at))
}

/// One bar per limit window that hasn't reset yet (with its name if `labels`), and how old the
/// numbers are.
fn limit_bars(ui: &mut egui::Ui, windows: &[status::Window], at: SystemTime, labels: bool) {
    let now = SystemTime::now();
    let mut shown = 0;
    for w in windows {
        let reset = w.resets_at.map(|t| UNIX_EPOCH + std::time::Duration::from_secs(t));
        if reset.is_some_and(|r| r <= now) {
            continue; // window has reset since the payload was written
        }
        shown += 1;
        if labels {
            ui.label(&w.label);
        }
        let text = limit_text(w.used, reset.and_then(|r| r.duration_since(now).ok()));
        ui.add(egui::ProgressBar::new((w.used / 100.0) as f32).text(text));
    }
    let age = now.duration_since(at).unwrap_or_default();
    let ago = if age.as_secs() < 60 { format!("{}s", age.as_secs()) } else { short_duration(age) };
    if shown == 0 && !windows.is_empty() {
        ui.label(RichText::new(format!("Reset since they were read {ago} ago; the next reading shows the new numbers.")).weak());
        return;
    }
    ui.label(RichText::new(format!("Updated {ago} ago")).weak().small());
}

/// 45% used, resets in 4h 7m → "45% used · 55% left · resets in 4h 7m"
fn limit_text(used: f64, resets_in: Option<std::time::Duration>) -> String {
    let used = used.clamp(0.0, 100.0);
    let text = format!("{used:.0}% used · {:.0}% left", 100.0 - used.round());
    match resets_in {
        Some(d) => format!("{text} · resets in {}", short_duration(d)),
        None => text,
    }
}

/// 14_200 → "13.9 KB"
fn file_size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// `ids` (a list's order) with `moving` (some of them, in any order) taken out and put
/// together, in `ids`' order, where the line before `ids[slot]` was.
fn dropped_order(ids: &[String], moving: &[String], slot: usize) -> Vec<String> {
    let moved = |id: &&String| moving.contains(id);
    let mut order: Vec<String> = ids[..slot.min(ids.len())].iter().filter(|id| !moved(id)).cloned().collect();
    order.extend(ids.iter().filter(moved).cloned());
    order.extend(ids[slot.min(ids.len())..].iter().filter(|id| !moved(id)).cloned());
    order
}

/// When a session row was last active, in local time: "14:32" today, "Nov 2 14:32" this
/// year, "Nov 2 2025" before.
pub(crate) fn row_time(t: SystemTime, now: SystemTime) -> String {
    fn local(t: SystemTime) -> (i32, u32, u32, u32, u32) {
        let secs = t.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as libc::time_t;
        // SAFETY: localtime_r only writes the tm it is given.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&secs, &mut tm) };
        (tm.tm_year + 1900, tm.tm_mon as u32, tm.tm_mday as u32, tm.tm_hour as u32, tm.tm_min as u32)
    }
    format_row_time(local(t), local(now))
}

/// `row_time` from local (year, month 0-11, day, hour, minute) of the time and of now.
fn format_row_time((y, mo, d, h, mi): (i32, u32, u32, u32, u32), (ny, nmo, nd, ..): (i32, u32, u32, u32, u32)) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = MONTHS[mo.min(11) as usize];
    if (y, mo, d) == (ny, nmo, nd) {
        format!("{h:02}:{mi:02}")
    } else if y == ny {
        format!("{month} {d} {h:02}:{mi:02}")
    } else {
        format!("{month} {d} {y}")
    }
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

/// The question sent to the forked session: the harness (Settings), then the current goal and
/// checklist for focus.
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

/// "-Users-me-Desktop-foo-bar" → "Desktop-foo-bar"
fn short_project(p: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default().replace(['/', '.'], "-");
    p.strip_prefix(&home).map(|s| s.trim_start_matches('-')).unwrap_or(p).to_owned()
}

impl eframe::App for App {
    /// Before every frame, and also while the window is hidden (behind others or minimized),
    /// when eframe draws nothing: runs, answers and commands keep being taken.
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.adopt_runs();
        self.poll_runner();
        self.poll_summary();
        self.poll_naming();
        self.poll_verdicts();
        self.updater.poll(&self.ctx);
        if std::mem::take(&mut self.updater.restart) {
            // Quits like Cmd+Q; the sessions it runs go on and the new scope picks them up.
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.poll_overviews();
        self.mark_seen();
        self.poll_orchestration();
        self.poll_control();
        self.poll_btw();
        self.poll_snapshots();
        self.poll_shared();
        self.poll_ai();
        self.poll_refine();
        self.poll_dictation();
        self.poll_usage();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.global_keys(&ctx);
        // Image files dropped on the window join the prompt.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| images::is_image_file(p)).collect());
        if !dropped.is_empty() && self.open.is_some() {
            self.insert_images(dropped.iter().map(|p| images::add_file(p)).collect());
        }
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
            if self.showing_overview() {
                self.overview(ui);
                return;
            }
            if self.showing_orchestration() {
                self.orchestration(ui);
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
            self.conversation_rect = None;
            match self.main_tab.filter(|&i| i < self.file_tabs.len()) {
                Some(i) => self.file_tab(ui, i),
                None => self.conversation(ui),
            }
            // A press in the conversation gives it the keyboard (for the arrows); one anywhere
            // else takes it away.
            let press = ui.input(|i| i.pointer.any_pressed().then_some(i.pointer.interact_pos()).flatten());
            let in_conversation = |p| self.conversation_rect.is_some_and(|r: egui::Rect| r.contains(p));
            if let Some(p) = press {
                let had = self.conversation_keys;
                self.conversation_keys = in_conversation(p) && !find_bar.is_some_and(|r: egui::Rect| r.contains(p));
                if self.conversation_keys {
                    ui.memory_mut(|m| m.stop_text_input());
                }
                if had != self.conversation_keys {
                    ui.ctx().request_repaint();
                }
            }
            // A plain click anywhere else in the main area goes to the input, like a terminal;
            // unless it went to another text field (refine's "Other…"), which keeps the focus.
            let area = ui.max_rect();
            let click = ui.input(|i| i.pointer.primary_clicked().then_some(i.pointer.interact_pos()).flatten()).filter(|&p| !in_conversation(p));
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
        self.confirm_trash_modal(&ctx);
        self.confirm_delete_message_modal(&ctx);
        self.model_prompt_modal(&ctx);
        // A Cmd+V the input didn't take (it wasn't focused) is over.
        images::paste_pressed();
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
    fn dropped_rows_go_together_where_the_line_was() {
        let ids: Vec<String> = ["a", "b", "c", "d"].map(String::from).to_vec();
        let drop = |moving: &[&str], slot| dropped_order(&ids, &moving.iter().map(|m| m.to_string()).collect::<Vec<_>>(), slot).join("");
        assert_eq!(drop(&["d"], 0), "dabc");
        assert_eq!(drop(&["a"], 4), "bcda", "below the last");
        assert_eq!(drop(&["a"], 2), "bacd");
        assert_eq!(drop(&["b"], 1), "abcd", "its own place");
        assert_eq!(drop(&["d", "a"], 2), "badc", "together, in list order");
        assert_eq!(drop(&["x"], 1), "abcd", "not in the list");
    }

    #[test]
    fn row_time_shows_the_time_today_and_the_date_before() {
        let now = (2026, 10, 3, 9, 0);
        assert_eq!(format_row_time((2026, 10, 3, 8, 5), now), "08:05");
        assert_eq!(format_row_time((2026, 10, 2, 23, 59), now), "Nov 2 23:59");
        assert_eq!(format_row_time((2025, 0, 15, 12, 0), now), "Jan 15 2025");
        assert!(!row_time(SystemTime::now(), SystemTime::now()).contains(' '), "now is today");
    }

    #[test]
    fn limit_text_shows_used_and_left() {
        assert_eq!(limit_text(45.0, Some(std::time::Duration::from_secs(4 * 3600 + 7 * 60))), "45% used · 55% left · resets in 4h 7m");
        assert_eq!(limit_text(3.4, None), "3% used · 97% left");
        assert_eq!(limit_text(104.0, None), "100% used · 0% left");
    }

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

    /// Regression: after Ctrl+C the partial reply showed twice (scope's copy and Claude's).
    #[test]
    fn a_saved_stop_hides_scopes_own_copy() {
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

    #[test]
    fn arrows_step_over_what_shows_no_card() {
        let m = |role, text: &str| session::Message { role, text: text.to_owned() };
        let ms = [
            m(Role::User, "one"),
            m(Role::Assistant, "[tool: Bash]"),
            m(Role::Assistant, "reply"),
            m(Role::User, "[Request interrupted by user]"),
            m(Role::User, "two"),
        ];
        assert_eq!(next_pick(&ms, Some(4), None, true), Some(2), "past a lone stop");
        assert_eq!(next_pick(&ms, Some(2), None, true), Some(0), "past tool lines");
        assert_eq!(next_pick(&ms, Some(0), None, true), None, "nothing above the first");
        assert_eq!(next_pick(&ms, Some(2), None, false), Some(4));
        assert_eq!(next_pick(&ms, Some(4), None, false), None);
        assert_eq!(next_pick(&ms, None, Some(2), true), Some(2), "Up first brings the top one's start in");
        assert_eq!(next_pick(&ms, None, Some(2), false), Some(4));
        assert_eq!(next_pick(&ms, None, None, true), Some(4));
    }

    #[test]
    fn copy_leaves_out_claudes_tool_lines() {
        let m = |role, text: &str| session::Message { role, text: text.into() };
        assert_eq!(copy_text(&m(Role::Assistant, "[tool: Read]\nFound it.\n[tool: Edit]\nFixed.")), "Found it.\nFixed.");
        assert_eq!(copy_text(&m(Role::User, "[tool: x] is literal here")), "[tool: x] is literal here");
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
            answer: String::new(),
        };
        let mut runs = HashMap::new();
        runs.insert("n1".to_owned(), run(true, "build it\nwith details"));
        runs.insert("old".to_owned(), run(false, "continue"));
        runs.insert("n2".to_owned(), run(true, "listed already"));
        let listed = vec![SessionMeta { id: "n2".into(), path: PathBuf::new(), project: String::new(), title: String::new(), modified: UNIX_EPOCH }];
        let pending = pending_new(&runs, &[], &listed, None);
        assert_eq!(pending, vec![("n1".to_owned(), "build it".to_owned(), "/tmp/play".to_owned())]);
        // In a workspace tab, only the sessions created there.
        assert!(pending_new(&runs, &[], &listed, Some(&["other".to_owned()])).is_empty());
        assert_eq!(pending_new(&runs, &[], &listed, Some(&["n1".to_owned()])).len(), 1);
        // Its run over, a new session stays until it is listed.
        runs.remove("n1");
        let ended = vec![("n1".to_owned(), "build it".to_owned(), None), ("n2".to_owned(), "listed already".to_owned(), None)];
        assert_eq!(pending_new(&runs, &ended, &listed, None), vec![("n1".to_owned(), "build it".to_owned(), String::new())]);
    }

    #[test]
    fn closing_a_tab_keeps_the_right_one_active() {
        assert_eq!(after_close(Some(2), 2), None);
        assert_eq!(after_close(Some(3), 1), Some(2));
        assert_eq!(after_close(Some(0), 1), Some(0));
        assert_eq!(after_close(None, 0), None);
    }
}
