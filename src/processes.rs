//! The orchestration page's **Processes** tab: everything the workspace's sessions have
//! started that is still running, as a tree under each session's `claude`.
//!
//! Every 2 s while the page is open, `ps` lists all processes. A session's `claude` is one
//! with `--resume <id>` / `--session-id <id>` of a session in the workspace (scope's own runs,
//! ones it adopted, or Claude Code in a terminal); side questions (`--fork-session`,
//! `--no-session-persistence`) don't count. Below it come its descendants, plus processes
//! that left the tree but kept its process group (scope's runs lead their own group).
//!
//! Each process is sorted into a kind: Claude Code's Bash tool (a shell sourcing its
//! `shell-snapshots/` file, then `eval '<command>'`), an MCP server, a ripgrep search (Grep
//! and Glob), or any other process. A Bash command is matched to its `Bash` tool call in the
//! session file, for Claude's description of it and its time limit (2 min unless the call
//! set one; none when run in the background). How far a command itself has got isn't known
//! to anyone but the command, so the progress shown is how much of that limit it has used.
use eframe::egui::{self, Color32, RichText, Stroke};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

/// How often the tab samples `ps`.
const EVERY: Duration = Duration::from_secs(2);
/// Claude Code's Bash tool stops a command after this long unless the call sets a timeout.
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;
/// How much of a session file's end is read for its Bash calls.
const TAIL: u64 = 512 * 1024;

/// One line of `ps -o pid=,ppid=,pgid=,%cpu=,rss=,etime=,args=`.
#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    /// Percent of one core (more than 100 for several threads busy at once).
    pub cpu: f32,
    pub rss_kb: u64,
    /// Seconds since it started.
    pub elapsed: u64,
    pub args: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Claude,
    Bash,
    Mcp,
    Search,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Claude => "claude",
            Kind::Bash => "Bash",
            Kind::Mcp => "MCP",
            Kind::Search => "search",
            Kind::Other => "process",
        }
    }
}

/// A Bash tool call in the session file.
#[derive(Clone, Debug, PartialEq)]
pub struct BashCall {
    pub command: String,
    pub description: Option<String>,
    pub timeout_ms: Option<u64>,
    pub background: bool,
    /// Its result is in (a background call's comes at once, while it keeps running).
    pub done: bool,
}

/// One process in a session's tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub pid: u32,
    /// 0 for the session's `claude`, 1 for what it started, and so on.
    pub depth: usize,
    pub kind: Kind,
    /// What it runs: the Bash command, or the command line made short.
    pub what: String,
    /// The whole command line, for its tooltip.
    pub args: String,
    pub cpu: f32,
    pub rss_kb: u64,
    pub elapsed: u64,
    /// It left the tree (its parent ended) but kept the session's process group.
    pub detached: bool,
    /// The Bash tool call it runs, when found.
    pub call: Option<BashCall>,
}

/// A session with something running.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub session: String,
    pub rows: Vec<Row>,
}

impl Group {
    pub fn cpu(&self) -> f32 {
        self.rows.iter().map(|r| r.cpu).sum()
    }
    pub fn rss_kb(&self) -> u64 {
        self.rows.iter().map(|r| r.rss_kb).sum()
    }
}

/// `ps` output in the form [`Proc`] describes.
pub fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines()
        .filter_map(|line| {
            let mut rest = line.trim_start();
            let mut field = || {
                let end = rest.find(char::is_whitespace)?;
                let (f, r) = rest.split_at(end);
                rest = r.trim_start();
                Some(f)
            };
            let pid = field()?.parse().ok()?;
            let ppid = field()?.parse().ok()?;
            let pgid = field()?.parse().ok()?;
            let cpu = field()?.replace(',', ".").parse().ok()?;
            let rss_kb = field()?.parse().ok()?;
            let elapsed = parse_etime(field()?)?;
            Some(Proc { pid, ppid, pgid, cpu, rss_kb, elapsed, args: rest.to_owned() })
        })
        // Ended, waiting for their parent to notice: nothing runs there any more.
        .filter(|p| p.args != "<defunct>")
        .collect()
}

/// `ps`'s elapsed time, `[[dd-]hh:]mm:ss`, in seconds.
pub fn parse_etime(s: &str) -> Option<u64> {
    let (days, clock) = match s.split_once('-') {
        Some((d, c)) => (d.parse::<u64>().ok()?, c),
        None => (0, s),
    };
    let mut secs = 0;
    for part in clock.split(':') {
        secs = secs * 60 + part.parse::<u64>().ok()?;
    }
    Some(days * 86_400 + secs)
}

/// The session a `claude` command line works on, unless it is a side question.
pub fn session_of(args: &str) -> Option<&str> {
    let words: Vec<&str> = args.split_whitespace().collect();
    let program = words.first()?;
    let claude = program.rsplit('/').next().is_some_and(|p| p.contains("claude"))
        || (matches!(program.rsplit('/').next(), Some("node" | "bun")) && words.get(1).is_some_and(|w| w.contains("claude")));
    if !claude || words.iter().any(|w| matches!(*w, "--fork-session" | "--no-session-persistence")) {
        return None;
    }
    words.windows(2).find(|w| matches!(w[0], "--resume" | "-r" | "--session-id")).map(|w| w[1])
}

/// The command Claude Code's Bash tool runs in this shell command line: what it `eval`s,
/// after sourcing its shell snapshot.
pub fn bash_command(args: &str) -> Option<String> {
    if !args.contains("shell-snapshots/snapshot-") {
        return None;
    }
    let at = args.find(" eval ")? + " eval ".len();
    let word = shell_word(&args[at..]);
    (!word.trim().is_empty()).then(|| word.trim().to_owned())
}

/// The first shell word of `s`, its quotes taken off (`'it'"'"'s'` is `it's`).
fn shell_word(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.trim_start().chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => out.extend(chars.by_ref().take_while(|&c| c != '\'')),
            '"' => {
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some(n @ ('"' | '\\' | '$' | '`')) => out.push(n),
                            Some(n) => {
                                out.push('\\');
                                out.push(n);
                            }
                            None => {}
                        },
                        c => out.push(c),
                    }
                }
            }
            '\\' => out.extend(chars.next()),
            c if c.is_whitespace() => break,
            c => out.push(c),
        }
    }
    out
}

fn kind_of(p: &Proc, depth: usize, parent_is_claude: bool) -> Kind {
    if depth == 0 {
        return Kind::Claude;
    }
    if bash_command(&p.args).is_some() {
        return Kind::Bash;
    }
    let program = p.args.split_whitespace().next().unwrap_or_default();
    let name = program.rsplit('/').next().unwrap_or_default();
    if name == "rg" || program.contains("/ripgrep/") {
        return Kind::Search;
    }
    let lower = p.args.to_ascii_lowercase();
    if parent_is_claude && (lower.contains("mcp") || lower.contains("modelcontextprotocol")) {
        return Kind::Mcp;
    }
    Kind::Other
}

/// A command line made short: each path down to its last part, at most 160 characters.
pub fn short(args: &str) -> String {
    let words: Vec<&str> = args
        .split_whitespace()
        .map(|w| if w.starts_with('/') && w.len() > 1 { w.trim_end_matches('/').rsplit('/').next().unwrap_or(w) } else { w })
        .collect();
    let s = words.join(" ");
    match s.char_indices().nth(160) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s,
    }
}

/// What `claude` itself is shown as: how it runs, and its model when given.
fn claude_what(args: &str) -> String {
    let words: Vec<&str> = args.split_whitespace().collect();
    let print = words.iter().any(|w| matches!(*w, "-p" | "--print"));
    let mut s = if print { "claude -p".to_owned() } else { "claude (terminal)".to_owned() };
    if let Some(w) = words.windows(2).find(|w| w[0] == "--model") {
        s.push_str(&format!(" · {}", w[1]));
    }
    s
}

/// The sessions' process trees, in `sessions`' order; sessions with nothing running are left
/// out. A session run twice at once (a terminal and scope) shows both trees.
pub fn groups(procs: &[Proc], sessions: &[String]) -> Vec<Group> {
    let mut children: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for p in procs {
        children.entry(p.ppid).or_default().push(p);
    }
    for list in children.values_mut() {
        list.sort_by_key(|p| p.pid);
    }
    let roots: Vec<(&str, &Proc)> = procs.iter().filter_map(|p| Some((session_of(&p.args)?, p))).collect();
    let root_pids: HashSet<u32> = roots.iter().map(|(_, p)| p.pid).collect();
    let mut out = Vec::new();
    for id in sessions {
        let mut rows = Vec::new();
        let mut mine = roots.iter().filter(|(s, _)| s == id).map(|(_, p)| *p).collect::<Vec<_>>();
        mine.sort_by_key(|p| p.pid);
        for root in mine {
            let mut seen = HashSet::new();
            // Depth first, children in pid order (roughly the order they started).
            let mut stack = vec![(root, 0usize, false)];
            while let Some((p, depth, parent_is_claude)) = stack.pop() {
                if !seen.insert(p.pid) {
                    continue;
                }
                rows.push(row(p, depth, parent_is_claude, false));
                for c in children.get(&p.pid).into_iter().flatten().rev() {
                    // Another session's claude started from here is listed under that session.
                    if !root_pids.contains(&c.pid) {
                        stack.push((c, depth + 1, depth == 0));
                    }
                }
            }
            if root.pgid == root.pid {
                for p in procs.iter().filter(|p| p.pgid == root.pid && !seen.contains(&p.pid) && !root_pids.contains(&p.pid)) {
                    rows.push(row(p, 1, false, true));
                }
            }
        }
        if !rows.is_empty() {
            out.push(Group { session: id.clone(), rows });
        }
    }
    out
}

fn row(p: &Proc, depth: usize, parent_is_claude: bool, detached: bool) -> Row {
    let kind = kind_of(p, depth, parent_is_claude);
    let what = match kind {
        Kind::Claude => claude_what(&p.args),
        Kind::Bash => bash_command(&p.args).unwrap_or_default(),
        _ => short(&p.args),
    };
    let args = if kind == Kind::Claude { short(&p.args) } else { p.args.clone() };
    Row { pid: p.pid, depth, kind, what, args, cpu: p.cpu, rss_kb: p.rss_kb, elapsed: p.elapsed, detached, call: None }
}

/// The Bash tool calls in session file lines, oldest first, and which have their result.
pub fn bash_calls(text: &str) -> Vec<BashCall> {
    let mut calls: Vec<(String, BashCall)> = Vec::new();
    let mut results = HashSet::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let Some(content) = v["message"]["content"].as_array() else { continue };
        for block in content {
            match block["type"].as_str() {
                Some("tool_use") if block["name"] == "Bash" => {
                    let input = &block["input"];
                    let Some(command) = input["command"].as_str() else { continue };
                    calls.push((
                        block["id"].as_str().unwrap_or_default().to_owned(),
                        BashCall {
                            command: command.trim().to_owned(),
                            description: input["description"].as_str().map(str::to_owned).filter(|d| !d.trim().is_empty()),
                            timeout_ms: input["timeout"].as_u64(),
                            background: input["run_in_background"].as_bool() == Some(true),
                            done: false,
                        },
                    ));
                }
                Some("tool_result") => {
                    if let Some(id) = block["tool_use_id"].as_str() {
                        results.insert(id.to_owned());
                    }
                }
                _ => {}
            }
        }
    }
    calls
        .into_iter()
        .map(|(id, mut c)| {
            c.done = !id.is_empty() && results.contains(&id);
            c
        })
        .collect()
}

/// A command as `ps` shows it: control characters (newlines, tabs) as `\ooo`.
fn ps_form(s: &str) -> String {
    s.chars().map(|c| if c.is_control() && (c as u32) < 0x80 { format!("\\{:03o}", c as u32) } else { c.to_string() }).collect()
}

/// Gives each Bash row the call it runs: the newest call still going (or in the background)
/// with the same command; else, when there is just one row and one call left, those two.
pub fn match_calls(rows: &mut [Row], calls: &[BashCall]) {
    let mut free: Vec<&BashCall> = calls.iter().filter(|c| !c.done || c.background).collect();
    let mut unmatched = Vec::new();
    for (i, r) in rows.iter_mut().enumerate().filter(|(_, r)| r.kind == Kind::Bash) {
        match free.iter().rposition(|c| ps_form(&c.command) == r.what) {
            Some(k) => {
                let c = free.remove(k).clone();
                // Its own text, with the lines ps shows as \012.
                r.what = c.command.clone();
                r.call = Some(c);
            }
            None => unmatched.push(i),
        }
    }
    let pending: Vec<&&BashCall> = free.iter().filter(|c| !c.done).collect();
    if let ([i], [c]) = (unmatched.as_slice(), pending.as_slice()) {
        rows[*i].what = c.command.clone();
        rows[*i].call = Some((**c).clone());
    }
}

/// The end of a session file, from its first whole line.
fn tail(path: &std::path::Path) -> String {
    let Ok(mut f) = std::fs::File::open(path) else { return String::new() };
    let len = f.metadata().map_or(0, |m| m.len());
    let start = len.saturating_sub(TAIL);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    let _ = f.read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    match (start > 0).then(|| text.find('\n')).flatten() {
        Some(i) => text[i + 1..].to_owned(),
        None => text,
    }
}

/// One sample: `ps`, the trees of `sessions` (id, session file), their Bash calls matched.
fn sample(sessions: &[(String, PathBuf)]) -> Vec<Group> {
    let out = Command::new("ps").args(["-axww", "-o", "pid=,ppid=,pgid=,%cpu=,rss=,etime=,args="]).stdin(Stdio::null()).output();
    let procs = out.map(|o| parse_ps(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    let ids: Vec<String> = sessions.iter().map(|(id, _)| id.clone()).collect();
    let mut groups = groups(&procs, &ids);
    for g in &mut groups {
        if !g.rows.iter().any(|r| r.kind == Kind::Bash) {
            continue;
        }
        let Some((_, path)) = sessions.iter().find(|(id, _)| *id == g.session) else { continue };
        if !path.as_os_str().is_empty() {
            match_calls(&mut g.rows, &bash_calls(&tail(path)));
        }
    }
    groups
}

/// The pids of row `i` and of everything listed under it.
pub fn subtree(rows: &[Row], i: usize) -> Vec<u32> {
    let depth = rows[i].depth;
    std::iter::once(rows[i].pid).chain(rows[i + 1..].iter().take_while(|r| r.depth > depth).map(|r| r.pid)).collect()
}

/// Ends processes: SIGTERM to each, then SIGKILL to those still there 2 s later.
pub fn terminate(pids: Vec<u32>) {
    for &pid in &pids {
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        for pid in pids {
            if unsafe { libc::kill(pid as i32, 0) } == 0 {
                unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            }
        }
    });
}

/// What the developer did on the tab.
pub enum Event {
    /// Open this session.
    Open(String),
    /// Stop this session's `claude` (its run, like Ctrl+C), the pid given.
    StopClaude(String, u32),
    /// End these processes (a row and what it started).
    Stop(Vec<u32>),
}

/// Samples in the background while the page asks for it, every [`EVERY`].
#[derive(Default)]
pub struct Monitor {
    /// Processes asked to stop, shown as stopping until a sample no longer lists them.
    pub stopping: HashSet<u32>,
    rx: Option<Receiver<Vec<Group>>>,
    asked: Option<Instant>,
    /// The latest sample, and the workspace it is of.
    pub last: Option<(String, Vec<Group>)>,
    workspace: String,
}

impl Monitor {
    /// Called every frame the orchestration page of `workspace` is shown.
    pub fn poll(&mut self, ctx: &egui::Context, workspace: &str, sessions: Vec<(String, PathBuf)>) {
        if self.workspace != workspace {
            self.workspace = workspace.to_owned();
            self.rx = None;
            self.asked = None;
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(groups) => {
                    let alive: HashSet<u32> = groups.iter().flat_map(|g| g.rows.iter().map(|r| r.pid)).collect();
                    self.stopping.retain(|p| alive.contains(p));
                    self.last = Some((workspace.to_owned(), groups));
                    self.rx = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.rx = None,
            }
        }
        if self.rx.is_none() && self.asked.is_none_or(|t| t.elapsed() >= EVERY) {
            self.asked = Some(Instant::now());
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(sample(&sessions));
                ctx.request_repaint();
            });
            self.rx = Some(rx);
        }
        ctx.request_repaint_after(EVERY);
    }

    /// The latest sample of `workspace`, if there is one yet.
    pub fn of(&self, workspace: &str) -> Option<&[Group]> {
        self.last.as_ref().filter(|(w, _)| w == workspace).map(|(_, g)| g.as_slice())
    }
}

/// "45 s", "3:07", "1:02:09", "2 d 3 h".
pub fn duration(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs} s"),
        60..3600 => format!("{}:{:02}", secs / 60, secs % 60),
        3600..86_400 => format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
        _ => format!("{} d {} h", secs / 86_400, secs / 3600 % 24),
    }
}

/// "840 KB", "312 MB", "1.4 GB".
pub fn memory(kb: u64) -> String {
    match kb {
        0..1024 => format!("{kb} KB"),
        1024..1_048_576 => format!("{} MB", kb / 1024),
        _ => format!("{:.1} GB", kb as f64 / 1_048_576.0),
    }
}

/// The share of its time limit a Bash command has used, and what to say about it; None for
/// anything without a limit.
pub fn limit_progress(r: &Row) -> Option<(f32, String)> {
    let call = r.call.as_ref()?;
    if call.background {
        return None;
    }
    let limit = call.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS) / 1000;
    if limit == 0 {
        return None;
    }
    let left = limit.saturating_sub(r.elapsed);
    let text = if left == 0 { format!("at its {} limit", duration(limit)) } else { format!("{} left of {}", duration(left), duration(limit)) };
    Some(((r.elapsed as f32 / limit as f32).min(1.0), text))
}

/// The Processes tab. `names` gives each session's name; `stopping`, the processes already
/// asked to stop.
pub fn ui(ui: &mut egui::Ui, groups: Option<&[Group]>, names: &HashMap<&str, &str>, stopping: &HashSet<u32>, accent: Color32) -> Option<Event> {
    let mut open = None;
    let pal = crate::theme::palette(ui.visuals());
    let Some(groups) = groups else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Looking for running processes…").weak());
        });
        return None;
    };
    let count: usize = groups.iter().map(|g| g.rows.len()).sum();
    let cpu: f32 = groups.iter().map(Group::cpu).sum();
    let rss: u64 = groups.iter().map(Group::rss_kb).sum();
    if groups.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Nothing is running in this scope's sessions.").weak());
            ui.label(
                RichText::new("While a session works, its claude and everything it started (Bash commands, MCP servers, searches) show here.")
                    .weak()
                    .small(),
            );
        });
        return None;
    }
    let plural = |n: usize, w: &str| match (n, w) {
        (1, _) => format!("1 {w}"),
        (_, "process") => format!("{n} processes"),
        _ => format!("{n} {w}s"),
    };
    ui.label(
        RichText::new(format!(
            "{} in {} · {:.0}% CPU · {} RAM · every 2 s",
            plural(count, "process"),
            plural(groups.len(), "session"),
            cpu,
            memory(rss)
        ))
        .weak(),
    );
    ui.add_space(6.0);
    egui::ScrollArea::vertical().id_salt("processes").auto_shrink(false).show(ui, |ui| {
        for g in groups {
            egui::Frame::new()
                .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
                .corner_radius(8.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let name = names.get(g.session.as_str()).copied().unwrap_or(g.session.as_str());
                        let link = ui.add(egui::Label::new(RichText::new(name).strong().color(accent)).sense(egui::Sense::click()));
                        if link.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open this session").clicked() {
                            open = Some(Event::Open(g.session.clone()));
                        }
                        ui.label(RichText::new(format!("{} · {:.0}% CPU · {}", plural(g.rows.len(), "process"), g.cpu(), memory(g.rss_kb()))).weak());
                    });
                    ui.add_space(4.0);
                    egui::Grid::new(("processes", &g.session)).striped(true).num_columns(6).spacing([14.0, 4.0]).show(ui, |ui| {
                        for h in ["What", "CPU", "RAM", "Running for", "Progress", ""] {
                            ui.label(RichText::new(h).small().weak());
                        }
                        ui.end_row();
                        for (i, r) in g.rows.iter().enumerate() {
                            process_row(ui, r, &pal);
                            if stopping.contains(&r.pid) {
                                ui.spinner().on_hover_text("Stopping…");
                            } else {
                                let hover = match r.kind {
                                    Kind::Claude => "Stop Claude in this session, like Ctrl+C: it and everything it started end, the queue pauses",
                                    Kind::Bash => "End this command and what it started; Claude sees it fail and carries on",
                                    _ => "End this process and what it started (SIGTERM, SIGKILL 2 s later if still there)",
                                };
                                let button = egui::Button::new(RichText::new("⏹").color(pal.error));
                                if ui.add(button).on_hover_text(hover).clicked() {
                                    open = Some(match r.kind {
                                        Kind::Claude => Event::StopClaude(g.session.clone(), r.pid),
                                        _ => Event::Stop(subtree(&g.rows, i)),
                                    });
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
            ui.add_space(8.0);
        }
    });
    open
}

fn process_row(ui: &mut egui::Ui, r: &Row, pal: &crate::theme::Palette) {
    let width = (ui.available_width() * 0.55).max(220.0);
    ui.horizontal(|ui| {
        ui.set_max_width(width);
        ui.add_space(r.depth as f32 * 14.0);
        if r.depth > 0 {
            ui.label(RichText::new("└").weak().monospace());
        }
        let color = match r.kind {
            Kind::Claude => pal.assistant,
            Kind::Bash => pal.user,
            Kind::Mcp => pal.note,
            Kind::Search => pal.add_fg,
            Kind::Other => ui.visuals().weak_text_color(),
        };
        egui::Frame::new().stroke(Stroke::new(1.0, color)).corner_radius(3.0).inner_margin(egui::Margin::symmetric(4, 0)).show(ui, |ui| {
            ui.label(RichText::new(r.kind.label()).small().color(color));
        });
        ui.vertical(|ui| {
            if let Some(d) = r.call.as_ref().and_then(|c| c.description.as_deref()) {
                ui.add(egui::Label::new(RichText::new(d).strong()).truncate());
            }
            let mut lines = r.what.lines();
            let first = lines.next().unwrap_or_default();
            let what = if lines.next().is_some() { format!("{first} …") } else { first.to_owned() };
            let mut what = RichText::new(what).monospace();
            if r.call.as_ref().is_some_and(|c| c.description.is_some()) {
                what = what.weak();
            }
            let full = if r.call.is_some() { &r.what } else { &r.args };
            let hover = format!("pid {}{}\n{}", r.pid, if r.detached { " · left its parent, still in the session's process group" } else { "" }, full);
            ui.add(egui::Label::new(what).truncate()).on_hover_text(hover);
        });
    });
    ui.label(RichText::new(format!("{:.1}%", r.cpu)).monospace()).on_hover_text("Of one core, as ps counts it (above 100% when several threads are busy)");
    ui.label(RichText::new(memory(r.rss_kb)).monospace()).on_hover_text("Resident memory");
    ui.label(RichText::new(duration(r.elapsed)).monospace());
    match (limit_progress(r), r.call.as_ref()) {
        (Some((share, text)), _) => {
            let bar = egui::ProgressBar::new(share).desired_width(150.0).text(text);
            ui.add(bar).on_hover_text(
                "Of the time limit Claude Code gives this Bash command (2 min unless the call sets one): it is stopped there. How far the command itself has got isn't known.",
            );
        }
        (None, Some(c)) if c.background => {
            ui.label(RichText::new("in the background").weak()).on_hover_text("A background Bash command has no time limit; it runs until it ends or is stopped");
        }
        _ => {
            ui.label(RichText::new("—").weak());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "5d0c3a3e-8a43-4f7e-9a59-0d2b8c1f7a10";

    fn ps_line(pid: u32, ppid: u32, pgid: u32, args: &str) -> String {
        format!("{pid:>6} {ppid:>6} {pgid:>6}   1.5  20480    01:02 {args}")
    }

    fn bash_shell(cmd: &str) -> String {
        format!(
            "/bin/zsh -c source /Users/me/.claude/shell-snapshots/snapshot-zsh-1-x.sh 2>/dev/null || true && setopt NO_EXTENDED_GLOB 2>/dev/null || true && eval {cmd} < /dev/null && pwd -P >| /tmp/claude-1-cwd"
        )
    }

    #[test]
    fn ps_lines_and_elapsed_times_parse() {
        let p = parse_ps("  412     1   412  12,5 314704 1-02:03:04 /usr/bin/claude -p --resume x\n bad line\n  413   412   412   0.0      0    00:01 <defunct>\n");
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].pid, p[0].ppid, p[0].pgid, p[0].cpu, p[0].rss_kb), (412, 1, 412, 12.5, 314_704));
        assert_eq!(p[0].elapsed, 86_400 + 2 * 3600 + 3 * 60 + 4);
        assert_eq!(p[0].args, "/usr/bin/claude -p --resume x");
        assert_eq!(parse_etime("00:32"), Some(32));
        assert_eq!(parse_etime("1:02:03"), Some(3723));
        assert_eq!(parse_etime("x"), None);
    }

    #[test]
    fn a_session_is_found_by_its_claude_but_not_by_side_questions() {
        assert_eq!(session_of(&format!("/Users/me/.local/bin/claude -p --append-system-prompt You are --resume {ID} --model opus")), Some(ID));
        assert_eq!(session_of(&format!("claude --session-id {ID}")), Some(ID));
        assert_eq!(session_of(&format!("node /opt/claude-code/cli.js -r {ID}")), Some(ID));
        assert_eq!(session_of(&format!("claude -p --resume {ID} --fork-session")), None);
        assert_eq!(session_of(&format!("vim --resume {ID}")), None);
    }

    #[test]
    fn the_bash_tools_command_comes_out_of_its_shell_unquoted() {
        assert_eq!(bash_command(&bash_shell("'cargo test 2>&1 | tail -5'")).as_deref(), Some("cargo test 2>&1 | tail -5"));
        assert_eq!(bash_command(&bash_shell(r#"'awk '"'"'{print $1}'"'"' f'"#)).as_deref(), Some("awk '{print $1}' f"));
        assert_eq!(bash_command(&bash_shell(r#""say \"hi\"""#)).as_deref(), Some(r#"say "hi""#));
        assert_eq!(bash_command("/bin/zsh -c eval 'ls'"), None, "not Claude Code's shell");
    }

    #[test]
    fn trees_hold_claudes_descendants_sorted_by_kind() {
        let text = [
            ps_line(100, 1, 100, &format!("/Users/me/.local/bin/claude -p --resume {ID} --model opus --output-format stream-json")),
            ps_line(101, 100, 100, "npx -y @modelcontextprotocol/server-github"),
            ps_line(102, 100, 100, &bash_shell("'cargo build'")),
            ps_line(103, 102, 100, "/Users/me/.cargo/bin/cargo build"),
            ps_line(104, 100, 100, "/opt/claude/vendor/ripgrep/arm64-darwin/rg --files"),
            ps_line(105, 1, 100, "/usr/bin/python3 -m http.server"),
            ps_line(200, 1, 200, "/usr/bin/claude -p --resume other"),
            ps_line(201, 200, 200, "sleep 5"),
            ps_line(300, 1, 300, "/bin/zsh"),
        ]
        .join("\n");
        let g = groups(&parse_ps(&text), &[ID.to_owned(), "nothing".to_owned()]);
        assert_eq!(g.len(), 1, "only sessions asked for, with something running");
        let rows: Vec<(u32, usize, Kind, &str, bool)> = g[0].rows.iter().map(|r| (r.pid, r.depth, r.kind, r.what.as_str(), r.detached)).collect();
        assert_eq!(
            rows,
            [
                (100, 0, Kind::Claude, "claude -p · opus", false),
                (101, 1, Kind::Mcp, "npx -y @modelcontextprotocol/server-github", false),
                (102, 1, Kind::Bash, "cargo build", false),
                (103, 2, Kind::Other, "cargo build", false),
                (104, 1, Kind::Search, "rg --files", false),
                (105, 1, Kind::Other, "python3 -m http.server", true),
            ]
        );
        assert_eq!(g[0].rss_kb(), 6 * 20480);
    }

    #[test]
    fn bash_rows_get_their_calls_description_and_time_limit() {
        let lines = [
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls","description":"List"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"cargo build ","description":"Build it","timeout":300000}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"npm run dev","run_in_background":true}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t3"}]}}"#,
        ]
        .join("\n");
        let calls = bash_calls(&lines);
        assert_eq!(calls.iter().map(|c| c.done).collect::<Vec<_>>(), [true, false, true]);
        let text = [
            ps_line(100, 1, 100, &format!("claude -p --resume {ID}")),
            ps_line(101, 100, 100, &bash_shell("'cargo build'")),
            ps_line(102, 100, 100, &bash_shell("'npm run dev'")),
            ps_line(103, 100, 100, &bash_shell("'ls'")),
        ]
        .join("\n");
        let mut g = groups(&parse_ps(&text), &[ID.to_owned()]);
        match_calls(&mut g[0].rows, &calls);
        let r = &g[0].rows;
        assert_eq!(r[1].call.as_ref().and_then(|c| c.description.as_deref()), Some("Build it"));
        assert_eq!(limit_progress(&r[1]).map(|p| p.1), Some("3:58 left of 5:00".to_owned()));
        assert!(r[2].call.as_ref().is_some_and(|c| c.background));
        assert_eq!(limit_progress(&r[2]), None, "no limit in the background");
        assert_eq!(r[3].call, None, "a finished call isn't one still running");
    }

    #[test]
    fn a_lone_command_claude_code_rewrote_still_finds_its_call() {
        let calls = bash_calls(r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"make"}}]}}"#);
        let text = [ps_line(100, 1, 100, &format!("claude -p --resume {ID}")), ps_line(101, 100, 100, &bash_shell("'make 2>&1'"))].join("\n");
        let mut g = groups(&parse_ps(&text), &[ID.to_owned()]);
        match_calls(&mut g[0].rows, &calls);
        assert_eq!(limit_progress(&g[0].rows[1]).map(|p| p.1), Some("58 s left of 2:00".to_owned()));
    }

    #[test]
    fn a_command_of_several_lines_matches_as_ps_shows_it() {
        let calls = bash_calls(r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cat <<EOF\nhi\tthere\nEOF"}},{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls"}}]}}"#);
        let text = [
            ps_line(100, 1, 100, &format!("claude -p --resume {ID}")),
            ps_line(101, 100, 100, &bash_shell(r"'cat <<EOF\012hi\011there\012EOF'")),
        ]
        .join("\n");
        let mut g = groups(&parse_ps(&text), &[ID.to_owned()]);
        match_calls(&mut g[0].rows, &calls);
        assert_eq!(g[0].rows[1].what, "cat <<EOF\nhi\tthere\nEOF", "shown as written, not as ps escapes it");
        assert!(g[0].rows[1].call.is_some());
    }

    #[test]
    fn stopping_a_row_takes_what_is_under_it() {
        let text = [
            ps_line(100, 1, 100, &format!("claude -p --resume {ID}")),
            ps_line(101, 100, 100, &bash_shell("'make'")),
            ps_line(102, 101, 100, "make"),
            ps_line(103, 102, 100, "cc a.c"),
            ps_line(104, 100, 100, "npx some-mcp-server"),
        ]
        .join("\n");
        let g = groups(&parse_ps(&text), &[ID.to_owned()]);
        assert_eq!(subtree(&g[0].rows, 1), [101, 102, 103]);
        assert_eq!(subtree(&g[0].rows, 3), [103]);
        assert_eq!(subtree(&g[0].rows, 4), [104]);
        assert_eq!(subtree(&g[0].rows, 0), [100, 101, 102, 103, 104]);
    }

    #[test]
    fn sizes_and_times_read_short() {
        assert_eq!(memory(900), "900 KB");
        assert_eq!(memory(314_704), "307 MB");
        assert_eq!(memory(1_500_000), "1.4 GB");
        assert_eq!(duration(45), "45 s");
        assert_eq!(duration(187), "3:07");
        assert_eq!(duration(3729), "1:02:09");
        assert_eq!(short("/usr/local/bin/node /a/b/server.js --port 3000"), "node server.js --port 3000");
    }
}
