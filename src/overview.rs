//! The overview shown in "All sessions": one card per workspace, in its color, with its scope of
//! work and the high-level Jira tickets it mentions, then every one of its sessions, newest
//! first: what it is doing, what it needs from the developer, and how recent it is. What the
//! helper model made of them is kept in `overview.json`, keyed by workspace name, and written
//! again for the sessions with new activity.
use crate::runner;
use crate::session;
use crate::theme;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, StrokeKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Ticket {
    pub key: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Summary {
    /// The workspace's scope of work, in a sentence.
    #[serde(default)]
    pub scope: String,
    /// Session id → what the helper model made of it.
    #[serde(default)]
    pub sessions: HashMap<String, Note>,
    #[serde(default)]
    pub tickets: Vec<Ticket>,
    /// Folder of the newest session, and its git branch, when it was summarized.
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    /// When it was written, and the newest session change it covers (Unix seconds).
    pub made: u64,
    pub newest: u64,
}

/// One session, as the overview shows it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Note {
    /// What it is doing, or last did.
    pub doing: String,
    /// A decision or action it needs from the developer; empty if none.
    #[serde(default)]
    pub needs: String,
    /// The session's newest change this covers (Unix seconds).
    pub at: u64,
}

/// Workspace name → its summary, saved as JSON.
#[derive(Default, Debug)]
pub struct Overviews(HashMap<String, Summary>);

impl Overviews {
    pub fn path() -> PathBuf {
        crate::settings::config_dir().join("overview.json")
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

    pub fn get(&self, name: &str) -> Option<&Summary> {
        self.0.get(name)
    }

    /// Keeps a renamed workspace's entry under its new name.
    pub fn rename(&mut self, old: &str, new: &str) {
        if let Some(v) = self.0.remove(old) {
            self.0.insert(new.to_owned(), v);
        }
    }

    pub fn set(&mut self, name: &str, s: Summary) {
        self.0.insert(name.to_owned(), s);
    }
}

/// Written again on its own when missing, or when a session that isn't working has activity
/// the summary hasn't seen and the summary is at least this old (so a busy workspace isn't
/// summarized after every reply). Working sessions are summarized once their run ends.
const REFRESH_AFTER: u64 = 3 * 60;

/// A session of the workspace, as the summarizer reads it.
#[derive(Clone, Debug)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub goal: Option<String>,
    pub path: PathBuf,
    /// Newest change (Unix seconds).
    pub modified: u64,
    pub running: bool,
}

impl Summary {
    /// Whether `src` has activity this summary hasn't seen (it is new, or changed since).
    fn has_news(&self, src: &Source) -> bool {
        self.sessions.get(&src.id).is_none_or(|n| src.modified > n.at)
    }
}

pub fn needs_summary(s: Option<&Summary>, sources: &[Source], now: u64) -> bool {
    match s {
        None => !sources.is_empty(),
        Some(s) if s.scope.is_empty() => !sources.is_empty(),
        Some(s) => sources.iter().any(|src| !src.running && s.has_news(src)) && now.saturating_sub(s.made) >= REFRESH_AFTER,
    }
}

pub fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Sessions whose conversation is read per summary (the newest with news), and how much of each.
const MAX_READ: usize = 10;
const PER_SESSION: usize = 2_000;

/// Summarizes a workspace on a thread: reads the sessions with news since `previous` (all of
/// them without one), asks the helper model, and keeps what it said of the others. Tickets
/// are kept only when they really appear in the sessions. `sources` is newest first.
pub fn start(ctx: egui::Context, sources: Vec<Source>, previous: Option<Summary>, newest: u64) -> Receiver<Result<Summary, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let previous = previous.filter(|p| !p.scope.is_empty());
        let mut text = String::new();
        let mut folder = None;
        let mut read = Vec::new();
        for (n, src) in sources.iter().enumerate() {
            let fresh = !src.running && previous.as_ref().is_none_or(|p| p.has_news(src)) && read.len() < MAX_READ;
            text.push_str(&format!("\n## Session {}: {}\n", n + 1, src.title));
            if let Some(g) = &src.goal {
                text.push_str(&format!("Goal: {}\n", g.lines().next().unwrap_or_default()));
            }
            let loaded = fresh.then(|| session::load(&src.path).ok()).flatten();
            if folder.is_none() {
                folder = loaded.as_ref().and_then(|s| s.cwd.clone());
            }
            match (loaded, previous.as_ref().and_then(|p| p.sessions.get(&src.id))) {
                (Some(s), _) => {
                    read.push(n);
                    text.push_str(&session::digest(&s.messages, 300, PER_SESSION));
                }
                (None, Some(note)) => {
                    text.push_str(&format!("(unchanged) Doing: {}\n", note.doing));
                    if !note.needs.is_empty() {
                        text.push_str(&format!("Needs: {}\n", note.needs));
                    }
                }
                (None, None) => text.push_str(if src.running { "(working, not read yet)\n" } else { "(not read)\n" }),
            }
        }
        let folder = folder.or_else(|| previous.as_ref().and_then(|p| p.folder.as_ref()).map(PathBuf::from));
        let branch = folder.as_deref().and_then(git_branch);
        let mut keys = jira_keys(&format!("{text}\n{}", branch.as_deref().unwrap_or_default()));
        for t in previous.iter().flat_map(|p| &p.tickets) {
            if !keys.contains(&t.key) {
                keys.push(t.key.clone());
            }
        }
        let numbers: Vec<usize> = read.iter().map(|n| n + 1).collect();
        let result = runner::ask_haiku(ctx.clone(), request(&text, previous.as_ref().map(|p| p.scope.as_str()), branch.as_deref(), &keys, &numbers))
            .recv()
            .unwrap_or_else(|_| Err("the helper model stopped unexpectedly".into()))
            .and_then(|reply| parse(&reply, &keys))
            .map(|reply| {
                let mut notes = HashMap::new();
                for (n, src) in sources.iter().enumerate() {
                    let said = reply.sessions.iter().find(|x| x.n == n + 1);
                    let old = previous.as_ref().and_then(|p| p.sessions.get(&src.id));
                    let note = match (read.contains(&n), said, old) {
                        (true, Some(x), _) => Note { doing: x.doing.clone(), needs: x.needs.clone(), at: src.modified },
                        // Read but not mentioned: not asked again until it changes.
                        (true, None, old) => Note { at: src.modified, ..old.cloned().unwrap_or_default() },
                        (false, _, Some(old)) => old.clone(),
                        (false, _, None) => continue,
                    };
                    notes.insert(src.id.clone(), note);
                }
                Summary {
                    scope: reply.scope,
                    sessions: notes,
                    tickets: reply.tickets,
                    folder: folder.map(|f| f.display().to_string()),
                    branch,
                    made: secs(SystemTime::now()),
                    newest,
                }
            });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

/// The request: the reply format, the previous scope, then every session, newest first:
/// those numbered in `read` with their conversation, the others with what was said of them.
pub fn request(sessions: &str, scope: Option<&str>, branch: Option<&str>, keys: &[String], read: &[usize]) -> String {
    let mut p = String::from(
        "You write the overview card of a WORKSPACE: a group of related Claude Code sessions \
         (a developer working with an AI coding agent). From the sessions below, reply with JSON \
         only, in this shape:\n\
         {\"scope\": \"the scope of work of the whole workspace, in one short sentence (at most 15 words)\",\n \
         \"sessions\": [{\"n\": <session number>, \"doing\": \"what it is doing, or last did\", \
         \"needs\": \"a decision, answer or action the developer must give or take for it to move on \
         (a question it asked, a choice to make, something to review or test); empty if none\"}],\n \
         \"tickets\": [{\"key\": \"ABC-123\", \"note\": \"2 to 6 words on what it is\"}]}\n\
         \"doing\" and \"needs\" are plain and concrete, each under 18 words; don't start with \"This session\" \
         or \"The session\". Read \"needs\" from the end of the conversation: only what is still open. \
         Write in the language the developer writes in.\n",
    );
    match read {
        [] => p.push_str("No session has news: \"sessions\" is [].\n"),
        r => p.push_str(&format!(
            "List in \"sessions\" only these sessions, whose conversation is shown: {}.\n",
            r.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
        )),
    }
    if let Some(s) = scope {
        p.push_str(&format!("The scope as written before (keep it unless the work has changed): {s}\n"));
    }
    if keys.is_empty() {
        p.push_str("No Jira tickets were mentioned: \"tickets\" is [].\n");
    } else {
        p.push_str(&format!(
            "Jira keys mentioned: {}. List at most 3, the highest-level first: epics and parent \
             tickets over stories and sub-tasks (say \"epic\" or \"parent\" in the note when the \
             sessions make that clear). Leave out keys that are clearly not Jira tickets.\n",
            keys.join(", ")
        ));
    }
    if let Some(b) = branch {
        p.push_str(&format!("Current git branch: {b}\n"));
    }
    p.push_str(&format!("\nSessions, newest first:\n{sessions}"));
    p
}

/// What the model said of one session, by its number in the request.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Said {
    pub n: usize,
    #[serde(default)]
    pub doing: String,
    #[serde(default)]
    pub needs: String,
}

#[derive(Debug, PartialEq)]
pub struct Reply {
    pub scope: String,
    pub sessions: Vec<Said>,
    pub tickets: Vec<Ticket>,
}

/// Reads the model's JSON, tolerating text or code fences around it. Tickets not in `keys`
/// (not seen in the sessions) are dropped.
pub fn parse(reply: &str, keys: &[String]) -> Result<Reply, String> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        scope: String,
        #[serde(default)]
        sessions: Vec<Said>,
        #[serde(default)]
        tickets: Vec<Ticket>,
    }
    let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}')) else {
        return Err(format!("unexpected reply: {}", reply.chars().take(200).collect::<String>()));
    };
    let raw: Raw = serde_json::from_str(&reply[start..=end]).map_err(|e| format!("unexpected reply ({e})"))?;
    if raw.scope.trim().is_empty() {
        return Err("the reply was empty".into());
    }
    let mut tickets: Vec<Ticket> = Vec::new();
    for t in raw.tickets {
        let key = t.key.trim().to_uppercase();
        if keys.contains(&key) && !tickets.iter().any(|x| x.key == key) {
            tickets.push(Ticket { key, note: t.note.trim().to_owned() });
        }
    }
    tickets.truncate(3);
    let sessions = raw
        .sessions
        .into_iter()
        .map(|s| Said { n: s.n, doing: s.doing.trim().to_owned(), needs: clean_needs(&s.needs) })
        .collect();
    Ok(Reply { scope: raw.scope.trim().to_owned(), sessions, tickets })
}

/// Models write "none" and the like for an empty "needs".
fn clean_needs(s: &str) -> String {
    let t = s.trim();
    let bare = t.trim_end_matches('.').to_lowercase();
    if ["", "none", "nothing", "n/a", "-", "no"].contains(&bare.as_str()) { String::new() } else { t.to_owned() }
}

/// Words shaped like Jira keys (`ABC-123`: 2 to 10 capitals or digits starting with a capital,
/// a dash, digits), in order of first appearance.
pub fn jira_keys(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let starts_word = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        if !(starts_word && b[i].is_ascii_uppercase()) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < b.len() && (b[j].is_ascii_uppercase() || b[j].is_ascii_digit()) {
            j += 1;
        }
        let prefix = j - i;
        if (2..=10).contains(&prefix) && j < b.len() && b[j] == b'-' {
            let mut k = j + 1;
            while k < b.len() && b[k].is_ascii_digit() {
                k += 1;
            }
            let digits = k - j - 1;
            let ends_word = k == b.len() || !b[k].is_ascii_alphanumeric();
            if (1..=7).contains(&digits) && ends_word {
                let key = text[i..k].to_owned();
                if seen.insert(key.clone()) {
                    out.push(key);
                }
                i = k;
                continue;
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// The branch checked out in `dir` (a worktree's `.git` file is followed), None if detached
/// or not a repository.
pub fn git_branch(dir: &Path) -> Option<String> {
    let dot_git = dir.ancestors().map(|d| d.join(".git")).find(|p| p.exists())?;
    let git_dir = if dot_git.is_file() {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let p = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
        if p.is_absolute() { p } else { dot_git.parent()?.join(p) }
    } else {
        dot_git
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    head.trim().strip_prefix("ref: refs/heads/").map(str::to_owned)
}

/// "just now", "5 min ago", "3 h ago", "2 d ago".
pub fn ago(then: u64, now: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", d / 60),
        3600..86_400 => format!("{} h ago", d / 3600),
        _ => format!("{} d ago", d / 86_400),
    }
}

/// "last interaction 5 min ago".
pub fn last_interaction(then: u64, now: u64) -> String {
    format!("last interaction {}", ago(then, now))
}

/// One workspace as the overview shows it.
pub struct Card<'a> {
    /// Its index among the workspaces (cards are not in their tabs' order).
    pub w: usize,
    pub name: &'a str,
    pub color: [u8; 3],
    pub running: usize,
    pub queued: usize,
    /// Its sessions, newest first.
    pub rows: Vec<Row<'a>>,
    pub summary: Option<&'a Summary>,
    pub busy: bool,
    pub error: Option<&'a str>,
}

/// One session in a workspace's card.
pub struct Row<'a> {
    pub id: &'a str,
    pub title: &'a str,
    /// Newest change (Unix seconds).
    pub modified: u64,
    pub running: bool,
    pub tool: Option<&'a str>,
    pub queued: usize,
    /// Its last run ended after it was last open: it is waiting to be looked at.
    pub finished: bool,
}

pub enum Event {
    Open(usize),
    /// Open this session, in workspace `.0`.
    OpenSession(usize, String),
    /// Done with this session: it leaves its workspace.
    Finish(String),
    Refresh(usize),
    RefreshAll,
}

/// What the overview needs to know besides the cards.
pub struct Options<'a> {
    /// Sessions not in any workspace.
    pub loose: usize,
    pub jira_url: &'a str,
    /// A session untouched this long (seconds) is offered to be finished; 0 = never.
    pub stale_after: u64,
}

/// Cards in the order the overview shows them: the workspace whose session went idle most
/// recently first (sessions still working don't count); those with none idle last.
pub fn sort(cards: &mut [Card]) {
    let idle = |c: &Card| c.rows.iter().filter(|r| !r.running).map(|r| r.modified).max().unwrap_or(0);
    cards.sort_by_key(|c| std::cmp::Reverse(idle(c)));
}

/// The whole overview: a header with totals, then the cards in as many columns as fit.
pub fn ui(ui: &mut egui::Ui, cards: &[Card], opts: &Options) -> Option<Event> {
    let mut event = None;
    let now = secs(SystemTime::now());
    if cards.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("No workspaces yet.\n\nCreate one with + at the top left, then drag sessions onto it.\nEach one gets a card here with what it is about.").weak());
        });
        return None;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading("Workspaces");
        let sessions: usize = cards.iter().map(|c| c.rows.len()).sum();
        let running: usize = cards.iter().map(|c| c.running).sum();
        let finished: usize = cards.iter().map(|c| c.rows.iter().filter(|r| r.finished).count()).sum();
        let mut totals = format!("{} · {}", plural(cards.len(), "workspace"), plural(sessions, "session"));
        if running > 0 {
            totals.push_str(&format!(" · {running} working"));
        }
        if finished > 0 {
            totals.push_str(&format!(" · {finished} finished"));
        }
        ui.label(RichText::new(totals).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let busy = cards.iter().any(|c| c.busy);
            if ui.add_enabled(!busy, egui::Button::new("↻ Refresh all").small()).on_hover_text("Summarize every workspace again with the helper model").clicked() {
                event = Some(Event::RefreshAll);
            }
        });
    });
    ui.add_space(8.0);
    egui::ScrollArea::vertical().id_salt("overview").auto_shrink(false).show(ui, |ui| {
        let columns = ((ui.available_width() / 460.0) as usize).clamp(1, 2);
        ui.columns(columns, |cols| {
            for (k, card) in cards.iter().enumerate() {
                if let Some(e) = card_ui(&mut cols[k % columns], card.w, card, now, opts) {
                    event = Some(e);
                }
            }
        });
        if opts.loose > 0 {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("{} not in a workspace.", plural(opts.loose, "session"))).weak().small());
        }
    });
    event
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

/// Yellow, readable on either theme: what needs the developer.
fn needs_color(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(235, 200, 60) } else { Color32::from_rgb(165, 120, 0) }
}

fn card_ui(ui: &mut egui::Ui, w: usize, card: &Card, now: u64, opts: &Options) -> Option<Event> {
    let mut event = None;
    let [r, g, b] = card.color;
    let color = Color32::from_rgb(r, g, b);
    let v = ui.visuals().clone();
    let frame = egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(r, g, b, if v.dark_mode { 14 } else { 22 }))
        .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(r, g, b, 110)))
        .corner_radius(10.0)
        .inner_margin(egui::Margin { left: 16, right: 12, top: 10, bottom: 10 });
    let shown = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Name, with its color dot; how big it is on the right.
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 5.0, color);
            let name = ui
                .add(egui::Label::new(RichText::new(card.name).strong().size(16.0)).sense(Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Open this workspace");
            if name.clicked() {
                event = Some(Event::Open(w));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(plural(card.rows.len(), "session")).weak().small());
                if card.queued > 0 {
                    ui.label(RichText::new(format!("+{} queued", card.queued)).weak().small());
                }
            });
        });
        let summary = card.summary;
        // Its scope of work, and the tickets it is about.
        match summary.filter(|s| !s.scope.is_empty()) {
            Some(s) => {
                ui.add(egui::Label::new(&s.scope).wrap());
                if !s.tickets.is_empty() {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        for t in &s.tickets {
                            ticket(ui, t, color, opts.jira_url);
                        }
                    });
                }
            }
            None if card.rows.is_empty() => {
                ui.label(RichText::new("No sessions yet. Drag some onto its tab.").weak());
            }
            None if card.busy => {}
            None => {
                ui.label(RichText::new("Not summarized yet.").weak());
            }
        }
        ui.add_space(6.0);
        for row in &card.rows {
            let note = summary.and_then(|s| s.sessions.get(row.id));
            if let Some(e) = row_ui(ui, w, row, note, now, opts.stale_after) {
                event = Some(e);
            }
            ui.add_space(4.0);
        }
        // Footer: how fresh, and refresh.
        ui.horizontal(|ui| {
            if card.busy {
                ui.spinner();
                ui.label(RichText::new("Summarizing…").weak().small());
            } else if let Some(e) = card.error {
                ui.label(RichText::new(format!("Could not summarize: {e}")).small().color(v.error_fg_color))
                    .on_hover_text(e);
            } else if let Some(s) = summary {
                ui.label(RichText::new(format!("Summarized {}", ago(s.made, now))).weak().small());
            }
            if !card.rows.is_empty() && !card.busy {
                let refresh = ui.add(egui::Button::new(RichText::new("↻").small()).frame(false)).on_hover_text("Summarize again");
                if refresh.clicked() {
                    event = Some(Event::Refresh(w));
                }
            }
        });
    });
    // The workspace's color as a stripe down the card's left edge, like its tab.
    let rect = shown.response.rect;
    let stripe = egui::Rect::from_min_max(rect.min + egui::vec2(5.0, 10.0), egui::pos2(rect.min.x + 8.0, rect.max.y - 10.0));
    ui.painter().rect_filled(stripe, 1.5, color);
    ui.add_space(10.0);
    event
}

/// A session: its name, what it is doing, what it needs from the developer (yellow), and at
/// the bottom right when it was last touched (red once stale, with **Finish session**) or a
/// spinner while it works. One whose run finished unseen is outlined and marked. Clicking it
/// opens it.
fn row_ui(ui: &mut egui::Ui, w: usize, row: &Row, note: Option<&Note>, now: u64, stale_after: u64) -> Option<Event> {
    let mut event = None;
    let pal = theme::palette(ui.visuals());
    let v = ui.visuals().clone();
    let stale = !row.running && stale_after > 0 && now.saturating_sub(row.modified) >= stale_after;
    let (fill, stroke) = if row.finished {
        let [r, g, b, _] = pal.add_fg.to_array();
        (Color32::from_rgba_unmultiplied(r, g, b, if v.dark_mode { 26 } else { 34 }), Stroke::new(1.5, pal.add_fg))
    } else {
        (v.faint_bg_color, Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color))
    };
    // Sensing before its contents, so the button on it still gets its clicks.
    let shown = ui.scope_builder(egui::UiBuilder::new().id_salt(("overview-row", row.id)).sense(Sense::click()), |ui| {
        ui.style_mut().interaction.selectable_labels = false;
        egui::Frame::new().fill(fill).stroke(stroke).corner_radius(7.0).inner_margin(egui::Margin { left: 10, right: 8, top: 6, bottom: 5 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let badge = if row.finished { 80.0 } else { 0.0 };
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - badge).max(40.0));
                    ui.add(egui::Label::new(RichText::new(row.title).strong()).truncate());
                });
                if row.finished {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("✓ Finished").small().strong().color(pal.add_fg))
                            .on_hover_text("Its last run ended after you last opened it");
                    });
                }
            });
            if let Some(tool) = row.tool.filter(|_| row.running) {
                ui.label(RichText::new(format!("⚙ {tool}…")).monospace().weak().small());
            }
            match note.filter(|n| !n.doing.is_empty()) {
                Some(n) => {
                    ui.add(egui::Label::new(&n.doing).wrap());
                }
                None => {
                    ui.label(RichText::new(if row.running { "Working…" } else { "Not summarized yet." }).weak());
                }
            }
            if let Some(needs) = note.map(|n| n.needs.as_str()).filter(|n| !n.is_empty()) {
                ui.add(egui::Label::new(RichText::new(format!("⚠ {needs}")).color(needs_color(v.dark_mode))).wrap());
            }
            let line = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            ui.allocate_ui_with_layout(line, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if row.running {
                    ui.label(RichText::new("working").weak().small());
                    ui.add(egui::Spinner::new().size(12.0));
                } else {
                    let when = RichText::new(last_interaction(row.modified, now)).small();
                    ui.label(if stale { when.color(pal.error) } else { when.weak() });
                    if stale && ui.small_button("Finish session").on_hover_text("Take it out of this workspace (the session itself is kept)").clicked() {
                        event = Some(Event::Finish(row.id.to_owned()));
                    }
                }
                if row.queued > 0 {
                    ui.label(RichText::new(format!("+{} queued", row.queued)).weak().small());
                }
            });
        })
    });
    let resp = shown.response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open this session");
    if resp.clicked() && event.is_none() {
        event = Some(Event::OpenSession(w, row.id.to_owned()));
    }
    event
}

/// A ticket chip: the key, outlined in the workspace color, and its note. A link when a Jira
/// URL is set in Settings.
fn ticket(ui: &mut egui::Ui, t: &Ticket, color: Color32, jira_url: &str) {
    let text = RichText::new(&t.key).monospace().small().strong().color(color);
    let base = jira_url.trim().trim_end_matches('/');
    let resp = if base.is_empty() {
        ui.add(egui::Label::new(text))
    } else {
        let link = ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand);
        if link.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(format!("{base}/browse/{}", t.key)));
        }
        link
    };
    ui.painter().rect_stroke(resp.rect.expand2(egui::vec2(4.0, 1.0)), 4.0, Stroke::new(1.0, color.gamma_multiply(0.6)), StrokeKind::Outside);
    ui.add_space(4.0);
    if !t.note.is_empty() {
        ui.label(RichText::new(&t.note).weak().small());
    }
    ui.add_space(10.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_jira_keys_once_in_order() {
        let t = "Fix PAY-12 (epic PAY-1), see feature/PAY-12-retry and AB-7.\nNot x-1, A-1, ABC-, abcPAY-9 or PAY-12x.";
        assert_eq!(jira_keys(t), vec!["PAY-12", "PAY-1", "AB-7"]);
    }

    #[test]
    fn parse_keeps_only_seen_tickets() {
        let reply = "```json\n{\"scope\":\" Ship payments \",\"sessions\":[{\"n\":1,\"doing\":\" Retries \",\"needs\":\"None.\"},{\"n\":2,\"doing\":\"x\",\"needs\":\"Pick a backoff\"}],\
                     \"tickets\":[{\"key\":\"pay-1\",\"note\":\"epic\"},{\"key\":\"FAKE-9\"},{\"key\":\"PAY-1\"}]}\n```";
        let r = parse(reply, &["PAY-1".into()]).unwrap();
        assert_eq!(r.scope, "Ship payments");
        assert_eq!(r.sessions[0], Said { n: 1, doing: "Retries".into(), needs: String::new() }, "\"None.\" is no need");
        assert_eq!(r.sessions[1].needs, "Pick a backoff");
        assert_eq!(r.tickets, vec![Ticket { key: "PAY-1".into(), note: "epic".into() }]);
        assert!(parse("no", &[]).is_err());
        assert!(parse("{\"scope\":\"\"}", &[]).is_err());
    }

    fn src(id: &str, modified: u64, running: bool) -> Source {
        Source { id: id.into(), title: id.into(), goal: None, path: PathBuf::new(), modified, running }
    }

    #[test]
    fn summarizes_when_missing_or_when_an_idle_session_has_news() {
        let note = Note { doing: "d".into(), needs: String::new(), at: 500 };
        let s = Summary { scope: "S".into(), sessions: HashMap::from([("a".into(), note)]), made: 1000, newest: 500, ..Default::default() };
        let late = 1000 + REFRESH_AFTER;
        assert!(needs_summary(None, &[src("a", 0, false)], 0));
        assert!(!needs_summary(None, &[], 0), "nothing to summarize");
        assert!(needs_summary(Some(&Summary { scope: String::new(), ..s.clone() }), &[src("a", 500, false)], 1000), "written before scopes existed");
        assert!(!needs_summary(Some(&s), &[src("a", 500, false)], late), "no new activity");
        assert!(!needs_summary(Some(&s), &[src("a", 600, false)], 1010), "too recent");
        assert!(!needs_summary(Some(&s), &[src("a", 600, true)], late), "still working");
        assert!(needs_summary(Some(&s), &[src("a", 600, false)], late));
        assert!(needs_summary(Some(&s), &[src("a", 500, false), src("b", 10, false)], late), "a session new to it");
    }

    #[test]
    fn request_lists_keys_sessions_read_and_the_old_scope() {
        let r = request("x", None, None, &[], &[]);
        assert!(r.contains("\"tickets\" is []") && r.contains("\"sessions\" is []"));
        let r = request("S", Some("Old scope"), Some("feat/PAY-1"), &["PAY-1".into()], &[1, 3]);
        assert!(r.contains("Jira keys mentioned: PAY-1") && r.contains("branch: feat/PAY-1") && r.ends_with("S"));
        assert!(r.contains("only these sessions, whose conversation is shown: 1, 3.") && r.contains("before (keep it unless the work has changed): Old scope"));
    }

    #[test]
    fn workspaces_are_sorted_by_their_latest_idle_session() {
        let row = |modified, running| Row { id: "x", title: "x", modified, running, tool: None, queued: 0, finished: false };
        let card = |w, rows| Card { w, name: "n", color: [0; 3], running: 0, queued: 0, rows, summary: None, busy: false, error: None };
        let mut cards = vec![card(0, vec![row(100, false)]), card(1, vec![row(900, true), row(50, false)]), card(2, vec![]), card(3, vec![row(300, false)])];
        sort(&mut cards);
        assert_eq!(cards.iter().map(|c| c.w).collect::<Vec<_>>(), [3, 0, 1, 2], "a working session doesn't count");
    }

    #[test]
    fn short_times() {
        assert_eq!(ago(100, 110), "just now");
        assert_eq!(ago(0, 7200), "2 h ago");
        assert_eq!(last_interaction(0, 300), "last interaction 5 min ago");
    }
}
