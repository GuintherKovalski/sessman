//! The overview shown in "All sessions": one card per workspace, in its color, with what the
//! helper model made of its sessions (purpose, what is being done now, where it is headed, and
//! the high-level Jira tickets it mentions). Summaries are kept in `overview.json`, keyed by
//! workspace name, and written again when the workspace has new activity.
use crate::runner;
use crate::session;
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
    pub purpose: String,
    pub now: String,
    pub heading: String,
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

    pub fn set(&mut self, name: &str, s: Summary) {
        self.0.insert(name.to_owned(), s);
    }
}

/// Written again on its own when missing, or when there was activity since and the summary
/// is at least this old (so a busy workspace isn't summarized after every reply).
const REFRESH_AFTER: u64 = 30 * 60;

pub fn needs_summary(s: Option<&Summary>, newest: u64, now: u64) -> bool {
    match s {
        None => true,
        Some(s) => newest > s.newest && now.saturating_sub(s.made) >= REFRESH_AFTER,
    }
}

pub fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A session of the workspace, as the summarizer reads it.
pub struct Source {
    pub title: String,
    pub goal: Option<String>,
    pub path: PathBuf,
}

/// Sessions read per workspace (the newest), and how much of each.
const MAX_SESSIONS: usize = 8;
const PER_SESSION: usize = 2_500;

/// Summarizes a workspace on a thread: reads its newest sessions, asks the helper model, and
/// keeps only tickets that really appear in them. `sources` is newest first.
pub fn start(ctx: egui::Context, sources: Vec<Source>, newest: u64) -> Receiver<Result<Summary, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let mut folder = None;
        for (n, src) in sources.iter().take(MAX_SESSIONS).enumerate() {
            let Ok(s) = session::load(&src.path) else { continue };
            if folder.is_none() {
                folder = s.cwd.clone();
            }
            text.push_str(&format!("\n## Session {}: {}\n", n + 1, src.title));
            if let Some(g) = &src.goal {
                text.push_str(&format!("Goal: {g}\n"));
            }
            text.push_str(&session::digest(&s.messages, 300, PER_SESSION));
        }
        let branch = folder.as_deref().and_then(git_branch);
        let keys = jira_keys(&format!("{text}\n{}", branch.as_deref().unwrap_or_default()));
        let result = runner::ask_haiku(ctx.clone(), request(&text, branch.as_deref(), &keys))
            .recv()
            .unwrap_or_else(|_| Err("the helper model stopped unexpectedly".into()))
            .and_then(|reply| parse(&reply, &keys))
            .map(|mut s| {
                s.folder = folder.map(|f| f.display().to_string());
                s.branch = branch;
                s.made = secs(SystemTime::now());
                s.newest = newest;
                s
            });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

pub fn request(sessions: &str, branch: Option<&str>, keys: &[String]) -> String {
    let mut p = String::from(
        "You write the overview card of a WORKSPACE: a group of related Claude Code sessions \
         (a developer working with an AI coding agent). From the sessions below, reply with JSON \
         only, in this shape:\n\
         {\"purpose\": \"what the workspace is for, the bigger reason behind the work\",\n \
         \"now\": \"what is being done most recently\",\n \
         \"heading\": \"where it is headed: the end state it is working towards\",\n \
         \"tickets\": [{\"key\": \"ABC-123\", \"note\": \"2 to 6 words on what it is\"}]}\n\
         Each sentence is plain, concrete and under 22 words; don't start with \"This workspace\". \
         Write in the language the developer writes in.\n",
    );
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

/// Reads the model's JSON, tolerating text or code fences around it. Tickets not in `keys`
/// (not seen in the sessions) are dropped.
pub fn parse(reply: &str, keys: &[String]) -> Result<Summary, String> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        purpose: String,
        #[serde(default)]
        now: String,
        #[serde(default)]
        heading: String,
        #[serde(default)]
        tickets: Vec<Ticket>,
    }
    let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}')) else {
        return Err(format!("unexpected reply: {}", reply.chars().take(200).collect::<String>()));
    };
    let raw: Raw = serde_json::from_str(&reply[start..=end]).map_err(|e| format!("unexpected reply ({e})"))?;
    if raw.purpose.trim().is_empty() && raw.now.trim().is_empty() {
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
    Ok(Summary {
        purpose: raw.purpose.trim().to_owned(),
        now: raw.now.trim().to_owned(),
        heading: raw.heading.trim().to_owned(),
        tickets,
        ..Default::default()
    })
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

/// One workspace as the overview shows it.
pub struct Card<'a> {
    pub name: &'a str,
    pub color: [u8; 3],
    pub sessions: usize,
    pub running: usize,
    pub queued: usize,
    /// Newest session change (Unix seconds), 0 when it has no sessions.
    pub newest: u64,
    pub summary: Option<&'a Summary>,
    pub busy: bool,
    pub error: Option<&'a str>,
}

pub enum Event {
    Open(usize),
    Refresh(usize),
    RefreshAll,
}

/// The whole overview: a header with totals, then the cards in as many columns as fit.
pub fn ui(ui: &mut egui::Ui, cards: &[Card], loose: usize, jira_url: &str) -> Option<Event> {
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
        let sessions: usize = cards.iter().map(|c| c.sessions).sum();
        let running: usize = cards.iter().map(|c| c.running).sum();
        let mut totals = format!("{} · {}", plural(cards.len(), "workspace"), plural(sessions, "session"));
        if running > 0 {
            totals.push_str(&format!(" · {running} working"));
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
        let columns = ((ui.available_width() / 380.0) as usize).clamp(1, 3);
        ui.columns(columns, |cols| {
            for (w, card) in cards.iter().enumerate() {
                if let Some(e) = card_ui(&mut cols[w % columns], w, card, now, jira_url) {
                    event = Some(e);
                }
            }
        });
        if loose > 0 {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("{} not in a workspace.", plural(loose, "session"))).weak().small());
        }
    });
    event
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

fn card_ui(ui: &mut egui::Ui, w: usize, card: &Card, now: u64, jira_url: &str) -> Option<Event> {
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
        // Name, with its color dot; how big and how recent on the right.
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
                let mut meta = plural(card.sessions, "session");
                if card.newest > 0 {
                    meta.push_str(&format!(" · {}", ago(card.newest, now)));
                }
                ui.label(RichText::new(meta).weak().small());
                if card.running > 0 {
                    ui.spinner().on_hover_text(format!("Claude is working in {}", plural(card.running, "session")));
                }
                if card.queued > 0 {
                    ui.label(RichText::new(format!("+{} queued", card.queued)).weak().small());
                }
            });
        });
        let summary = card.summary;
        // Where: folder and branch.
        if let Some(s) = summary.filter(|s| s.folder.is_some() || s.branch.is_some()) {
            let mut place = s.folder.as_deref().map(short_home).unwrap_or_default();
            if let Some(b) = &s.branch {
                place.push_str(&format!("{}⎇ {b}", if place.is_empty() { "" } else { "  " }));
            }
            ui.label(RichText::new(place).monospace().weak().small());
        }
        ui.add_space(6.0);
        match summary {
            Some(s) => {
                field(ui, "Purpose", &s.purpose, color);
                field(ui, "Now", &s.now, color);
                field(ui, "Heading", &s.heading, color);
                if !s.tickets.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for t in &s.tickets {
                            ticket(ui, t, color, jira_url);
                        }
                    });
                }
            }
            None if card.sessions == 0 => {
                ui.label(RichText::new("No sessions yet. Drag some onto its tab.").weak());
            }
            None if card.busy => {}
            None => {
                ui.label(RichText::new("Not summarized yet.").weak());
            }
        }
        // Footer: how fresh, and refresh.
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if card.busy {
                ui.spinner();
                ui.label(RichText::new("Summarizing…").weak().small());
            } else if let Some(e) = card.error {
                ui.label(RichText::new(format!("Could not summarize: {e}")).small().color(v.error_fg_color))
                    .on_hover_text(e);
            } else if let Some(s) = summary {
                let mut when = format!("Summarized {}", ago(s.made, now));
                if card.newest > s.newest {
                    when.push_str(" · new activity since");
                }
                ui.label(RichText::new(when).weak().small());
            }
            if card.sessions > 0 && !card.busy {
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

/// A small colored label, then the text wrapping beside it.
fn field(ui: &mut egui::Ui, label: &str, text: &str, color: Color32) {
    if text.is_empty() {
        return;
    }
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(62.0, ui.text_style_height(&egui::TextStyle::Body)), Sense::hover());
        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, label.to_uppercase(), egui::FontId::proportional(10.0), color);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(text).wrap());
        });
    });
    ui.add_space(2.0);
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

/// "/Users/me/Desktop/foo" → "~/Desktop/foo"
fn short_home(p: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match p.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => p.to_owned(),
    }
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
        let reply = "```json\n{\"purpose\":\" Ship payments \",\"now\":\"Retries\",\"heading\":\"GA\",\
                     \"tickets\":[{\"key\":\"pay-1\",\"note\":\"epic\"},{\"key\":\"FAKE-9\"},{\"key\":\"PAY-1\"}]}\n```";
        let s = parse(reply, &["PAY-1".into()]).unwrap();
        assert_eq!((s.purpose.as_str(), s.now.as_str()), ("Ship payments", "Retries"));
        assert_eq!(s.tickets, vec![Ticket { key: "PAY-1".into(), note: "epic".into() }]);
        assert!(parse("no", &[]).is_err());
        assert!(parse("{\"purpose\":\"\"}", &[]).is_err());
    }

    #[test]
    fn summarizes_when_missing_or_stale_and_old_enough() {
        let s = Summary { made: 1000, newest: 500, ..Default::default() };
        assert!(needs_summary(None, 0, 0));
        assert!(!needs_summary(Some(&s), 500, 1000 + REFRESH_AFTER), "no new activity");
        assert!(!needs_summary(Some(&s), 600, 1010), "too recent");
        assert!(needs_summary(Some(&s), 600, 1000 + REFRESH_AFTER));
    }

    #[test]
    fn request_lists_keys_or_says_none() {
        assert!(request("x", None, &[]).contains("\"tickets\" is []"));
        let r = request("S", Some("feat/PAY-1"), &["PAY-1".into()]);
        assert!(r.contains("Jira keys mentioned: PAY-1") && r.contains("branch: feat/PAY-1") && r.ends_with("S"));
    }

    #[test]
    fn short_times() {
        assert_eq!(ago(100, 110), "just now");
        assert_eq!(ago(0, 7200), "2 h ago");
    }
}
