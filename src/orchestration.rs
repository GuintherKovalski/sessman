//! Workspace orchestration: one page per workspace, opened from the bubble at the top of its
//! session list, to see how all of its sessions are doing. Live facts (who is working, queued
//! prompts, last activity, goal checklists) come straight from the app; on top of them the
//! helper model, led by the orchestration harness from Settings, writes a report: overall
//! progress, where each session stands, what needs the developer, and what comes next.
//!
//! Reports are kept per workspace in `orchestration.json` and updated incrementally: an update
//! sends the previous report and only the sessions with new activity since. **Clear &
//! re-summarize** forgets the report, its questions and what it covered, and starts over from
//! every session's digest.
use crate::markdown;
use crate::overview::{ago, secs};
use crate::runner;
use crate::session;
use crate::theme;
use eframe::egui::{self, Color32, Id, Key, Modifiers, RichText, Sense, Stroke, StrokeKind};
use egui_commonmark::CommonMarkCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::SystemTime;

/// What the orchestrator is told to be, editable in Settings → Harness. The reply format and
/// the sessions themselves are added after it.
pub const DEFAULT_HARNESS: &str = "You are the orchestrator of a WORKSPACE: a group of related Claude Code \
sessions in which a developer works with AI coding agents in parallel. Give the developer a clear sense of \
progress and of what is happening across all of them: what each session is doing, which ones are blocked or \
waiting for the developer, what is done, and what should happen next. Be concrete and brief, each sentence \
under 20 words, and write in the language the developer writes in.";

/// Stands for the orchestration page in a workspace's "last open" slot, so the tab reopens on it.
pub const LAST_KEY: &str = "@orchestration";

/// Sessions read per update (the newest), and how much of each.
const MAX_SESSIONS: usize = 12;
const PER_SESSION: usize = 3_000;
/// Questions and answers kept per workspace.
const MAX_LOG: usize = 20;

/// Where one session stands, as the orchestrator sees it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Status {
    pub id: String,
    /// "working", "waiting", "blocked", "done" or "idle".
    pub state: String,
    pub now: String,
    #[serde(default)]
    pub next: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Report {
    /// How far the workspace as a whole is towards its goals, 0..=100.
    pub progress: u8,
    pub summary: String,
    pub sessions: Vec<Status>,
    /// What needs the developer now.
    pub attention: Vec<String>,
    /// Next steps for the workspace as a whole.
    pub next: Vec<String>,
}

/// A question asked on the page, with the orchestrator's answer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Exchange {
    pub question: String,
    pub answer: String,
    pub at: u64,
}

/// One workspace's orchestration: its report and what it covers, and the questions asked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Orchestration {
    pub report: Option<Report>,
    /// When the report was last written (Unix seconds), and how many updates it has had since
    /// it was last cleared.
    pub made: u64,
    pub updates: u32,
    /// Session id → its newest change (Unix seconds) the report has seen.
    pub covered: HashMap<String, u64>,
    pub log: Vec<Exchange>,
}

impl Orchestration {
    /// Sessions with activity the report hasn't seen (new ones included).
    pub fn stale(&self, members: &[Member]) -> bool {
        members.iter().any(|m| self.covered.get(&m.id).is_none_or(|&c| m.modified > c))
    }

    /// Takes a finished update: a full one starts the count again.
    pub fn apply(&mut self, u: Update, now: u64) {
        if u.full {
            self.covered.clear();
            self.updates = 0;
        }
        self.covered.extend(u.covered);
        self.report = Some(u.report);
        self.made = now;
        self.updates += 1;
    }

    pub fn answered(&mut self, question: String, answer: String, now: u64) {
        self.log.push(Exchange { question, answer, at: now });
        if self.log.len() > MAX_LOG {
            self.log.remove(0);
        }
    }
}

/// Workspace name → its orchestration, saved as JSON.
#[derive(Default, Debug)]
pub struct Store(HashMap<String, Orchestration>);

impl Store {
    pub fn path() -> PathBuf {
        crate::settings::config_dir().join("orchestration.json")
    }

    pub fn load(path: &Path) -> Self {
        Self(std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&self.0)?)
    }

    pub fn get(&self, name: &str) -> Option<&Orchestration> {
        self.0.get(name)
    }

    pub fn entry(&mut self, name: &str) -> &mut Orchestration {
        self.0.entry(name.to_owned()).or_default()
    }

    pub fn clear(&mut self, name: &str) {
        self.0.remove(name);
    }
}

/// Updates and questions in flight, by workspace name, and the last failure of each (not
/// retried on its own until asked).
#[derive(Default)]
pub struct Jobs {
    pub updating: HashMap<String, Receiver<Result<Update, String>>>,
    pub asking: HashMap<String, (String, Receiver<Result<String, String>>)>,
    pub errors: HashMap<String, String>,
    pub ask_errors: HashMap<String, String>,
    /// Text in the page's question field.
    pub question: String,
}

/// A session of the workspace, as the orchestrator reads it.
#[derive(Clone, Debug)]
pub struct Member {
    pub id: String,
    pub title: String,
    /// Raw goal text with its checklist (goal.rs), if summarized.
    pub goal: Option<String>,
    pub path: PathBuf,
    /// Newest change (Unix seconds).
    pub modified: u64,
    pub running: bool,
    pub queued: usize,
}

/// A finished update: the new report, the sessions it saw (id → newest change), and whether
/// it started from scratch.
pub struct Update {
    pub report: Report,
    pub covered: HashMap<String, u64>,
    pub full: bool,
}

/// Writes the report on a thread: from scratch without a `previous` one, else an update of it
/// with only the sessions that changed since. `members` is newest first.
pub fn update(ctx: egui::Context, harness: String, model: String, members: Vec<Member>, previous: Option<Orchestration>) -> Receiver<Result<Update, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let members: Vec<Member> = members.into_iter().take(MAX_SESSIONS).collect();
        let previous = previous.filter(|p| p.report.is_some());
        let now = secs(SystemTime::now());
        let mut digests = Vec::new();
        let mut covered = HashMap::new();
        for m in &members {
            let fresh = previous.as_ref().is_none_or(|p| p.covered.get(&m.id).is_none_or(|&c| m.modified > c));
            let digest = fresh.then(|| session::load(&m.path).ok()).flatten().map(|s| session::digest(&s.messages, 300, PER_SESSION));
            if fresh {
                covered.insert(m.id.clone(), m.modified);
            }
            digests.push(digest);
        }
        let prev = previous.as_ref().and_then(|p| p.report.as_ref());
        let ids: Vec<String> = members.iter().map(|m| m.id.clone()).collect();
        let result = runner::ask_model(ctx.clone(), &model, request(&harness, &members, &digests, prev, now))
            .recv()
            .unwrap_or_else(|_| Err("the model stopped unexpectedly".into()))
            .and_then(|reply| parse(&reply, &ids))
            .map(|report| Update { report, covered, full: previous.is_none() });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

/// The update request: the harness, the reply format, the previous report (sessions by their
/// number in this list), then every session with its live status and, when it has news, its
/// conversation digest.
pub fn request(harness: &str, members: &[Member], digests: &[Option<String>], previous: Option<&Report>, now: u64) -> String {
    let mut p = format!("{}\n\n", harness.trim());
    p.push_str(
        "Reply with JSON only, in this shape:\n\
         {\"progress\": <0 to 100: how far the workspace as a whole is towards its goals>,\n \
         \"summary\": \"2 to 4 sentences on the overall state\",\n \
         \"sessions\": [{\"n\": <session number>, \"state\": \"working|waiting|blocked|done|idle\", \
         \"now\": \"what it is doing, or last did\", \"next\": \"its next step, or empty\"}],\n \
         \"attention\": [\"what needs the developer now; [] if nothing\"],\n \
         \"next\": [\"up to 4 next steps for the workspace as a whole\"]}\n\
         List every session below by its number. \"waiting\" means it needs the developer's input or \
         review; \"blocked\" means it hit a problem it could not solve; \"done\" means its goal is met.\n",
    );
    if let Some(prev) = previous {
        let numbered: Vec<serde_json::Value> = prev
            .sessions
            .iter()
            .filter_map(|s| {
                let n = members.iter().position(|m| m.id == s.id)? + 1;
                Some(serde_json::json!({"n": n, "state": s.state, "now": s.now, "next": s.next}))
            })
            .collect();
        let json = serde_json::json!({
            "progress": prev.progress,
            "summary": prev.summary,
            "sessions": numbered,
            "attention": prev.attention,
            "next": prev.next,
        });
        p.push_str(&format!(
            "\nYour PREVIOUS REPORT is below. Update it: keep what still holds, change what the new \
             activity changes.\n{json}\n"
        ));
    }
    p.push_str("\nSESSIONS, newest first:\n");
    for (n, (m, digest)) in members.iter().zip(digests).enumerate() {
        p.push_str(&format!("\n## Session {}: {}\n", n + 1, m.title));
        let mut status = vec![if m.running { "Claude is working in it right now".to_owned() } else { "not running".to_owned() }];
        if m.queued > 0 {
            status.push(format!("{} prompts queued", m.queued));
        }
        status.push(format!("last activity {}", ago(m.modified, now)));
        p.push_str(&format!("Status: {}\n", status.join(" · ")));
        if let Some(g) = &m.goal {
            p.push_str(&format!("Goal and checklist:\n{}\n", g.trim()));
        }
        match digest {
            Some(d) => p.push_str(&format!("Conversation digest:\n{d}")),
            None => p.push_str("(no new activity since the previous report)\n"),
        }
    }
    p
}

/// Reads the model's JSON, tolerating text or code fences around it. Sessions are matched back
/// to `ids` by number; unknown numbers and repeats are dropped.
pub fn parse(reply: &str, ids: &[String]) -> Result<Report, String> {
    #[derive(Deserialize)]
    struct RawStatus {
        n: serde_json::Value,
        #[serde(default)]
        state: String,
        #[serde(default)]
        now: String,
        #[serde(default)]
        next: String,
    }
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        progress: Option<f64>,
        #[serde(default)]
        summary: String,
        #[serde(default)]
        sessions: Vec<RawStatus>,
        #[serde(default)]
        attention: Vec<String>,
        #[serde(default)]
        next: Vec<String>,
    }
    let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}')) else {
        return Err(format!("unexpected reply: {}", reply.chars().take(200).collect::<String>()));
    };
    let raw: Raw = serde_json::from_str(&reply[start..=end]).map_err(|e| format!("unexpected reply ({e})"))?;
    if raw.summary.trim().is_empty() && raw.sessions.is_empty() {
        return Err("the reply was empty".into());
    }
    let mut sessions: Vec<Status> = Vec::new();
    for s in raw.sessions {
        let n = s.n.as_u64().or_else(|| s.n.as_str().and_then(|t| t.trim().parse().ok())).unwrap_or(0) as usize;
        let Some(id) = n.checked_sub(1).and_then(|i| ids.get(i)) else { continue };
        if sessions.iter().any(|x| &x.id == id) {
            continue;
        }
        let state = s.state.trim().to_lowercase();
        let state = if STATES.contains(&state.as_str()) { state } else { "idle".to_owned() };
        sessions.push(Status { id: id.clone(), state, now: s.now.trim().to_owned(), next: s.next.trim().to_owned() });
    }
    let list = |v: Vec<String>| v.into_iter().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect::<Vec<_>>();
    Ok(Report {
        progress: raw.progress.unwrap_or(0.0).clamp(0.0, 100.0).round() as u8,
        summary: raw.summary.trim().to_owned(),
        sessions,
        attention: list(raw.attention),
        next: list(raw.next).into_iter().take(4).collect(),
    })
}

const STATES: [&str; 5] = ["working", "waiting", "blocked", "done", "idle"];

/// Answers a question about the workspace on a thread, from the report, recent questions and a
/// short digest of each session.
pub fn ask(ctx: egui::Context, harness: String, model: String, members: Vec<Member>, orch: Option<Orchestration>, question: String) -> Receiver<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut p = format!(
            "{}\n\nAnswer the developer's QUESTION about this workspace, briefly, in Markdown. Use the \
             report and the sessions below; say so when they don't tell.\n",
            harness.trim()
        );
        if let Some(r) = orch.as_ref().and_then(|o| o.report.as_ref()) {
            p.push_str(&format!("\nCURRENT REPORT:\n{}\n", serde_json::to_string(r).unwrap_or_default()));
        }
        let recent: Vec<&Exchange> = orch.as_ref().map(|o| o.log.iter().rev().take(3).rev().collect()).unwrap_or_default();
        if !recent.is_empty() {
            p.push_str("\nEARLIER QUESTIONS:\n");
            for e in recent {
                let short: String = e.answer.chars().take(600).collect();
                p.push_str(&format!("Q: {}\nA: {short}\n", e.question));
            }
        }
        p.push_str("\nSESSIONS, newest first:\n");
        for (n, m) in members.iter().take(MAX_SESSIONS).enumerate() {
            p.push_str(&format!("\n## Session {}: {} (id {})\n", n + 1, m.title, m.id));
            if let Some(g) = &m.goal {
                p.push_str(&format!("Goal and checklist:\n{}\n", g.trim()));
            }
            if let Ok(s) = session::load(&m.path) {
                p.push_str(&session::digest(&s.messages, 200, 1_500));
            }
        }
        p.push_str(&format!("\nQUESTION: {question}\n"));
        let result = runner::ask_model(ctx.clone(), &model, p).recv().unwrap_or_else(|_| Err("the model stopped unexpectedly".into()));
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

/// Whether the page should update on its own: never written yet, or there is activity it hasn't
/// seen and the last update is at least `every` minutes old (0 = only when asked).
pub fn due(o: Option<&Orchestration>, members: &[Member], every: u32, now: u64) -> bool {
    if members.is_empty() {
        return false;
    }
    match o.filter(|o| o.report.is_some()) {
        None => true,
        Some(o) => every > 0 && o.stale(members) && now.saturating_sub(o.made) >= u64::from(every) * 60,
    }
}

/// The orchestration bubble at the top of a workspace's session list: shaped and colored like
/// the session bubbles, set apart only by a bold name and a thicker outline.
pub fn bubble(ui: &mut egui::Ui, selected: bool, line: &str, busy: bool) -> egui::Response {
    const PAD: egui::Vec2 = egui::vec2(10.0, 6.0);
    const SPIN: f32 = 12.0;
    let width = ui.available_width();
    let text_width = (width - 2.0 * PAD.x - if busy { SPIN + 6.0 } else { 0.0 }).max(10.0);
    let v = ui.visuals().clone();
    let title = ui.painter().layout_no_wrap("Orchestration".to_owned(), egui::TextStyle::Body.resolve(ui.style()), v.text_color());
    let sub = ui.painter().layout(line.to_owned(), egui::TextStyle::Small.resolve(ui.style()), v.weak_text_color(), text_width);
    let height = title.size().y + 2.0 + sub.size().y + 2.0 * PAD.y;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            v.selection.bg_fill
        } else if resp.hovered() {
            v.widgets.hovered.weak_bg_fill
        } else {
            v.faint_bg_color
        };
        let stroke_color = if selected { v.selection.stroke.color } else { v.widgets.noninteractive.bg_stroke.color };
        let painter = ui.painter();
        painter.rect(rect, 10.0, fill, Stroke::new(2.5, stroke_color), StrokeKind::Inside);
        let text_color = if selected { v.selection.stroke.color } else { v.strong_text_color() };
        let title_h = title.size().y;
        let pos = rect.min + PAD;
        // Bold: the name drawn twice, a hair apart (the app has no bold font).
        painter.galley(pos, title.clone(), text_color);
        painter.galley(pos + egui::vec2(0.6, 0.0), title, text_color);
        painter.galley(egui::pos2(pos.x, pos.y + title_h + 2.0), sub, v.weak_text_color());
        if busy {
            let spin = egui::Rect::from_center_size(egui::pos2(rect.right() - PAD.x - SPIN / 2.0, rect.center().y), egui::vec2(SPIN, SPIN));
            egui::Spinner::new().size(SPIN).paint_at(ui, spin);
        }
    }
    ui.add_space(4.0);
    resp.on_hover_text("Workspace orchestration: progress and what is happening in all its sessions")
}

/// One session as the page shows it: live facts from the app.
pub struct Live<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub running: bool,
    /// The tool it is starting, while running.
    pub tool: Option<&'a str>,
    pub queued: usize,
    pub modified: u64,
    /// Goal checklist (done, total), if summarized.
    pub checklist: Option<(usize, usize)>,
}

/// Everything the page shows.
pub struct Page<'a> {
    pub name: &'a str,
    pub color: [u8; 3],
    pub sessions: &'a [Live<'a>],
    pub orch: Option<&'a Orchestration>,
    pub busy: bool,
    pub error: Option<&'a str>,
    /// The question being answered, if any, and the last failure to answer.
    pub asking: Option<&'a str>,
    pub ask_error: Option<&'a str>,
    pub model: &'a str,
}

pub enum Event {
    /// Open this session.
    Open(String),
    Update,
    /// Clear the report and questions, and summarize from scratch.
    Clear,
    Ask(String),
    /// Forget the question and answer at this index.
    Forget(usize),
}

/// The orchestration page: header with actions, overall progress and report, a card per
/// session, and questions to the orchestrator at the bottom.
pub fn ui(ui: &mut egui::Ui, page: &Page, question: &mut String, md: &mut CommonMarkCache) -> Option<Event> {
    let mut event = None;
    let now = secs(SystemTime::now());
    let pal = theme::palette(ui.visuals());
    let [r, g, b] = page.color;
    let accent = Color32::from_rgb(r, g, b);
    let report = page.orch.and_then(|o| o.report.as_ref());

    // Header: name and counts on the left, actions on the right.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
        let c = dot.center();
        let d = 6.0;
        ui.painter().add(egui::Shape::convex_polygon(
            vec![c + egui::vec2(0.0, -d), c + egui::vec2(d, 0.0), c + egui::vec2(0.0, d), c + egui::vec2(-d, 0.0)],
            accent,
            Stroke::NONE,
        ));
        ui.heading(page.name);
        ui.label(RichText::new("orchestration").color(accent));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can = !page.busy && !page.sessions.is_empty();
            let clear = egui::Button::new("Clear & re-summarize");
            if ui
                .add_enabled(can, clear)
                .on_hover_text("Forget the report, what it has seen and the questions below, and summarize every session again from scratch")
                .clicked()
            {
                event = Some(Event::Clear);
            }
            if ui.add_enabled(can, egui::Button::new("↻ Update")).on_hover_text("Update the report with the activity since the last one").clicked() {
                event = Some(Event::Update);
            }
        });
    });
    let running = page.sessions.iter().filter(|s| s.running).count();
    let queued: usize = page.sessions.iter().map(|s| s.queued).sum();
    let mut counts = vec![plural(page.sessions.len(), "session")];
    if running > 0 {
        counts.push(format!("{running} working"));
    }
    if queued > 0 {
        counts.push(format!("{queued} queued"));
    }
    ui.label(RichText::new(counts.join(" · ")).weak());
    ui.add_space(6.0);

    if page.sessions.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("No sessions in this workspace yet.\n\nStart one with + New session, or drag sessions onto its tab.\nThis page then shows how they are all doing.").weak());
        });
        return None;
    }

    // Questions to the orchestrator: answers stack above the field, at the bottom of the page.
    let mut ask = false;
    let field_id = Id::new(("orchestration-question", page.name));
    if ui.memory(|m| m.has_focus(field_id)) && !question.trim().is_empty() && page.asking.is_none() {
        ask = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    }
    egui::Panel::bottom("orchestration-ask")
        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
        .resizable(false)
        .show(ui, |ui| {
            if let Some(q) = page.asking {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(format!("Asking: {q}")).weak().italics());
                });
            }
            if let Some(e) = page.ask_error {
                ui.colored_label(pal.error, format!("Could not answer: {e}"));
            }
            egui::Frame::new()
                .fill(pal.prompt_bg)
                .stroke(Stroke::new(1.0, accent.gamma_multiply(0.7)))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("?").monospace().strong().color(accent));
                        let hint = "Ask the orchestrator about this workspace · Enter to send (Shift+Enter: new line)";
                        let edit = egui::TextEdit::multiline(question)
                            .id(field_id)
                            .frame(egui::Frame::NONE)
                            .hint_text(hint)
                            .desired_rows(1)
                            .return_key(Some(egui::KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)))
                            .desired_width(ui.available_width());
                        ui.add_enabled(page.asking.is_none(), edit);
                    });
                });
        });
    if ask {
        event = Some(Event::Ask(std::mem::take(question).trim().to_owned()));
    }

    egui::ScrollArea::vertical().id_salt(("orchestration", page.name)).auto_shrink(false).show(ui, |ui| {
        // The report: progress, summary, what needs the developer, next steps.
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(r, g, b, if ui.visuals().dark_mode { 18 } else { 26 }))
            .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match report {
                    Some(rep) => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("PROGRESS").size(10.0).color(accent));
                            let bar = egui::ProgressBar::new(f32::from(rep.progress) / 100.0).fill(accent).text(format!("{}%", rep.progress));
                            ui.add(bar);
                        });
                        ui.add_space(6.0);
                        if !rep.summary.is_empty() {
                            ui.add(egui::Label::new(&rep.summary).wrap());
                        }
                        if !rep.attention.is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new("NEEDS YOU").size(10.0).strong().color(pal.note));
                            for a in &rep.attention {
                                ui.add(egui::Label::new(RichText::new(format!("•  {a}")).color(pal.note)).wrap());
                            }
                        }
                        if !rep.next.is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new("NEXT").size(10.0).color(accent));
                            for n in &rep.next {
                                ui.add(egui::Label::new(format!("•  {n}")).wrap());
                            }
                        }
                    }
                    None if page.busy => {
                        ui.label(RichText::new("Reading every session to write the first report…").weak());
                    }
                    None => {
                        ui.label(RichText::new("No report yet. Click ↻ Update to have the orchestrator read every session.").weak());
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if page.busy {
                        ui.spinner();
                        ui.label(RichText::new("Updating…").weak().small());
                    } else if let Some(e) = page.error {
                        ui.add(egui::Label::new(RichText::new(format!("Could not update: {e}")).small().color(pal.error)).truncate()).on_hover_text(e);
                    } else if let Some(o) = page.orch.filter(|o| o.report.is_some()) {
                        let mut when = format!("Updated {}", ago(o.made, now));
                        if o.updates > 1 {
                            when.push_str(&format!(" · {} updates since the last clear", o.updates - 1));
                        }
                        ui.label(RichText::new(when).weak().small());
                    }
                    ui.label(RichText::new(format!("· {}", page.model)).weak().small());
                });
            });
        ui.add_space(10.0);

        // A card per session: live state first, then what the report says about it.
        ui.label(RichText::new("Sessions").strong());
        ui.add_space(4.0);
        let columns = ((ui.available_width() / 340.0) as usize).clamp(1, 3);
        ui.columns(columns, |cols| {
            for (k, s) in page.sessions.iter().enumerate() {
                let status = report.and_then(|r| r.sessions.iter().find(|x| x.id == s.id));
                if session_card(&mut cols[k % columns], s, status, now, accent) {
                    event = Some(Event::Open(s.id.to_owned()));
                }
            }
        });

        // Questions asked so far, oldest first.
        if let Some(o) = page.orch.filter(|o| !o.log.is_empty()) {
            ui.add_space(6.0);
            ui.label(RichText::new("Questions").strong());
            ui.add_space(4.0);
            for (i, e) in o.log.iter().enumerate() {
                ui.push_id(("exchange", i), |ui| {
                    egui::Frame::new()
                        .fill(ui.visuals().faint_bg_color)
                        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.5)))
                        .inner_margin(8.0)
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_top(|ui| {
                                let text = (ui.available_width() - 24.0).max(0.0);
                                ui.allocate_ui_with_layout(egui::vec2(text, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.add(egui::Label::new(RichText::new(&e.question).italics().strong()).wrap());
                                });
                                if ui.small_button("×").on_hover_text("Forget this question").clicked() {
                                    event = Some(Event::Forget(i));
                                }
                            });
                            markdown::show(ui, md, &e.answer);
                            ui.label(RichText::new(ago(e.at, now)).weak().small());
                        });
                });
                ui.add_space(4.0);
            }
        }
    });
    event
}

/// A session's card; true when clicked (to open it).
fn session_card(ui: &mut egui::Ui, s: &Live, status: Option<&Status>, now: u64, accent: Color32) -> bool {
    let pal = theme::palette(ui.visuals());
    let state = if s.running { "working" } else { status.map_or("", |x| x.state.as_str()) };
    let state_color = match state {
        "working" => pal.user,
        "waiting" => pal.note,
        "blocked" => pal.error,
        "done" => pal.add_fg,
        _ => ui.visuals().weak_text_color(),
    };
    let shown = egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
        .corner_radius(8.0)
        .inner_margin(egui::Margin { left: 14, right: 10, top: 8, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if s.running {
                    ui.spinner();
                }
                let title_width = (ui.available_width() - 70.0).max(40.0);
                ui.scope(|ui| {
                    ui.set_max_width(title_width);
                    ui.add(egui::Label::new(RichText::new(s.title).strong()).truncate());
                });
                if !state.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let chip = ui.label(RichText::new(state).small().strong().color(state_color));
                        ui.painter().rect_stroke(chip.rect.expand2(egui::vec2(5.0, 1.0)), 6.0, Stroke::new(1.0, state_color.gamma_multiply(0.7)), StrokeKind::Outside);
                    });
                }
            });
            if let Some(tool) = s.tool.filter(|_| s.running) {
                ui.label(RichText::new(format!("⚙ {tool}…")).monospace().weak().small());
            }
            if let Some(st) = status {
                if !st.now.is_empty() {
                    ui.add(egui::Label::new(&st.now).wrap());
                }
                if !st.next.is_empty() {
                    ui.add(egui::Label::new(RichText::new(format!("Next: {}", st.next)).weak()).wrap());
                }
            }
            if let Some((done, total)) = s.checklist.filter(|&(_, t)| t > 0) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Goal {done}/{total}")).small().weak());
                    ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_height(5.0).fill(accent.gamma_multiply(0.8)));
                });
            }
            let mut meta = vec![ago(s.modified, now)];
            if s.queued > 0 {
                meta.push(format!("+{} queued", s.queued));
            }
            ui.label(RichText::new(meta.join(" · ")).weak().small());
        });
    // The card's own stripe, in its state's color.
    let rect = shown.response.rect;
    let stripe = egui::Rect::from_min_max(rect.min + egui::vec2(4.0, 8.0), egui::pos2(rect.min.x + 7.0, rect.max.y - 8.0));
    ui.painter().rect_filled(stripe, 1.5, state_color);
    let click = ui.interact(rect, Id::new(("orchestration-card", s.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open this session");
    ui.add_space(8.0);
    click.clicked()
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, modified: u64) -> Member {
        Member { id: id.into(), title: format!("T {id}"), goal: None, path: PathBuf::new(), modified, running: false, queued: 0 }
    }

    #[test]
    fn parse_maps_numbers_to_sessions_and_cleans_up() {
        let ids = vec!["a".to_owned(), "b".to_owned()];
        let reply = "```json\n{\"progress\": 140, \"summary\": \" Going well \", \"sessions\": [\
                     {\"n\": 2, \"state\": \"Blocked\", \"now\": \"tests fail\"},\
                     {\"n\": \"1\", \"state\": \"sleeping\", \"now\": \"x\", \"next\": \"ship\"},\
                     {\"n\": 9, \"state\": \"done\"}, {\"n\": 2, \"state\": \"done\"}],\
                     \"attention\": [\"review b\", \" \"], \"next\": [\"1\",\"2\",\"3\",\"4\",\"5\"]}\n```";
        let r = parse(reply, &ids).unwrap();
        assert_eq!(r.progress, 100);
        assert_eq!(r.summary, "Going well");
        assert_eq!(r.sessions.len(), 2);
        assert_eq!((r.sessions[0].id.as_str(), r.sessions[0].state.as_str()), ("b", "blocked"));
        assert_eq!((r.sessions[1].id.as_str(), r.sessions[1].state.as_str(), r.sessions[1].next.as_str()), ("a", "idle", "ship"));
        assert_eq!(r.attention, vec!["review b"]);
        assert_eq!(r.next.len(), 4);
        assert!(parse("nope", &ids).is_err());
        assert!(parse("{\"summary\": \"\"}", &ids).is_err());
    }

    #[test]
    fn request_has_harness_previous_report_and_only_new_digests() {
        let members = vec![member("a", 100), member("b", 50)];
        let prev = Report { progress: 40, summary: "S".into(), sessions: vec![Status { id: "b".into(), state: "done".into(), now: "N".into(), next: String::new() }], ..Default::default() };
        let p = request("MY HARNESS", &members, &[Some("USER: hi\n".into()), None], Some(&prev), 160);
        assert!(p.starts_with("MY HARNESS\n"));
        assert!(p.contains("PREVIOUS REPORT") && p.contains("\"n\":2"), "{p}");
        assert!(p.contains("## Session 1: T a\nStatus: not running · last activity 1 min ago\nConversation digest:\nUSER: hi"));
        assert!(p.contains("## Session 2: T b") && p.contains("(no new activity since the previous report)"));
        let first = request("H", &members, &[Some("x".into()), Some("y".into())], None, 160);
        assert!(!first.contains("PREVIOUS REPORT"));
    }

    #[test]
    fn updates_are_due_when_missing_or_stale_and_old_enough() {
        let members = vec![member("a", 100)];
        assert!(due(None, &members, 10, 0));
        assert!(!due(None, &[], 10, 0), "nothing to report on");
        let mut o = Orchestration { report: Some(Report::default()), made: 1000, ..Default::default() };
        o.covered.insert("a".into(), 100);
        assert!(!due(Some(&o), &members, 10, 5000), "nothing new");
        let newer = vec![member("a", 200)];
        assert!(!due(Some(&o), &newer, 10, 1000 + 599), "too soon");
        assert!(due(Some(&o), &newer, 10, 1000 + 600));
        assert!(!due(Some(&o), &newer, 0, 99_999), "0 = only when asked");
        assert!(due(Some(&o), &[member("a", 100), member("new", 1)], 10, 1600), "a new session is news");
    }

    #[test]
    fn a_full_update_restarts_what_it_covers_and_the_count() {
        let mut o = Orchestration::default();
        let up = |full, id: &str| Update { report: Report::default(), covered: [(id.to_owned(), 5)].into(), full };
        o.apply(up(true, "a"), 10);
        o.apply(up(false, "b"), 20);
        assert_eq!((o.updates, o.made, o.covered.len()), (2, 20, 2));
        o.apply(up(true, "c"), 30);
        assert_eq!((o.updates, o.covered.len()), (1, 1));
        for i in 0..(MAX_LOG + 3) {
            o.answered(format!("q{i}"), "a".into(), 0);
        }
        assert_eq!(o.log.len(), MAX_LOG);
        assert_eq!(o.log[0].question, "q3");
    }

    #[test]
    fn store_roundtrips_and_clears() {
        let path = std::env::temp_dir().join(format!("sessman-orch-{}.json", std::process::id()));
        let mut s = Store::default();
        s.entry("W").answered("q".into(), "a".into(), 1);
        s.save(&path).unwrap();
        let mut back = Store::load(&path);
        assert_eq!(back.get("W"), s.get("W"));
        back.clear("W");
        assert!(back.get("W").is_none());
        std::fs::remove_file(path).unwrap();
    }
}
