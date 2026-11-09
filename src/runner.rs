use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex, RwLock};

/// Model and effort picked for one session when it was made; empty = what Settings and
/// Claude Code's own settings give. Passed on every run of that session.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Choice {
    pub model: String,
    pub effort: String,
    /// `--permission-mode`.
    pub permissions: String,
    /// "on" / "off": `alwaysThinkingEnabled`.
    pub thinking: String,
    /// `--setting-sources`, e.g. "user" or "project,local".
    pub sources: String,
    /// "none": no MCP servers.
    pub mcp: String,
    /// "off": no skills or slash commands.
    pub slash: String,
    /// `--agent`: the agent the session runs as.
    pub agent: String,
    /// "off": no hooks.
    pub hooks: String,
    /// Plugins to switch off, comma-separated.
    pub plugins: String,
}

impl Choice {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The arguments this choice adds to a run.
    pub fn args(&self) -> Vec<String> {
        let mut args: Vec<String> = Vec::new();
        let mut pair = |k: &str, v: &str| {
            if !v.is_empty() {
                args.extend([k.to_owned(), v.to_owned()]);
            }
        };
        pair("--effort", &self.effort);
        pair("--permission-mode", &self.permissions);
        pair("--setting-sources", &self.sources);
        pair("--agent", &self.agent);
        let mut settings = serde_json::Map::new();
        if !self.thinking.is_empty() {
            settings.insert("alwaysThinkingEnabled".into(), Value::Bool(self.thinking == "on"));
        }
        if self.hooks == "off" {
            settings.insert("disableAllHooks".into(), Value::Bool(true));
        }
        let off: serde_json::Map<String, Value> = self.plugins.split(',').map(str::trim).filter(|p| !p.is_empty()).map(|p| (p.to_owned(), Value::Bool(false))).collect();
        if !off.is_empty() {
            settings.insert("enabledPlugins".into(), Value::Object(off));
        }
        if !settings.is_empty() {
            args.extend(["--settings".into(), Value::Object(settings).to_string()]);
        }
        if self.mcp == "none" {
            args.extend(["--strict-mcp-config".into(), "--mcp-config".into(), r#"{"mcpServers":{}}"#.into()]);
        }
        if self.slash == "off" {
            args.push("--disable-slash-commands".into());
        }
        args
    }
}

/// How to run Claude Code, from the Settings page.
#[derive(Clone, Debug, PartialEq)]
pub struct Claude {
    pub bin: String,
    /// For prompts; empty = Claude Code's default.
    pub model: String,
    /// For search, goals and refine.
    pub helper: String,
    pub extra: Vec<String>,
}

impl Default for Claude {
    fn default() -> Self {
        Self { bin: "claude".into(), model: String::new(), helper: "haiku".into(), extra: Vec::new() }
    }
}

impl Claude {
    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        let or = |v: &str, d: &str| if v.trim().is_empty() { d.to_owned() } else { v.trim().to_owned() };
        Self {
            bin: or(&s.claude_path, "claude"),
            model: s.model.trim().to_owned(),
            helper: or(&s.helper_model, "haiku"),
            extra: s.extra_args.split_whitespace().map(str::to_owned).collect(),
        }
    }
}

/// Where CLIs like `claude` usually live but a Finder-launched app's PATH
/// (`/usr/bin:/bin:/usr/sbin:/sbin`) does not reach.
const EXTRA_DIRS: [&str; 8] = [
    "~/.local/bin",
    "~/.claude/local",
    "~/.npm-global/bin",
    "~/.bun/bin",
    "~/.volta/bin",
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "~/.cargo/bin",
];

fn expand_home(p: &str, home: &str) -> String {
    match p.strip_prefix('~') {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("{home}{rest}"),
        _ => p.to_owned(),
    }
}

/// `current`, then the login shell's PATH, then [`EXTRA_DIRS`], without repeats.
pub fn merged_path(current: &str, login: &str, home: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let extra = EXTRA_DIRS.iter().map(|d| expand_home(d, home));
    for dir in current.split(':').chain(login.split(':')).map(str::to_owned).chain(extra) {
        if !dir.is_empty() && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out.join(":")
}

/// The command to spawn for `bin`: a path as is (with `~` expanded), a bare name as the first
/// executable of that name on `path`, or the name unchanged when there is none.
pub fn resolve_bin(bin: &str, path: &str, home: &str) -> PathBuf {
    let bin = expand_home(bin.trim(), home);
    if bin.contains('/') {
        return PathBuf::from(bin);
    }
    use std::os::unix::fs::PermissionsExt;
    path.split(':')
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(expand_home(d, home)).join(&bin))
        .find(|p| p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .unwrap_or_else(|| PathBuf::from(bin))
}

/// PATH as the user's login shell sets it, or empty if it does not answer within 3 s.
fn login_shell_path() -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let Ok(mut child) = Command::new(shell)
        .args(["-ilc", "printf '\\n__SCOPE_PATH__%s\\n' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };
    let mut stdout = child.stdout.take().unwrap();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        let _ = tx.send(s);
    });
    let out = rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap_or_default();
    let _ = child.kill();
    let _ = child.wait();
    out.lines().find_map(|l| l.strip_prefix("__SCOPE_PATH__")).unwrap_or_default().to_owned()
}

/// Gives this process (and so every `claude` run and the tools it starts) the PATH a terminal
/// would have. Call first thing in `main`, before any thread starts.
pub fn fix_path() {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let path = merged_path(&current, &login_shell_path(), &home);
    // SAFETY: called at the start of main, while this is the only thread.
    unsafe { std::env::set_var("PATH", path) };
}

static CLAUDE: RwLock<Option<Claude>> = RwLock::new(None);

pub fn configure(c: Claude) {
    *CLAUDE.write().unwrap() = Some(c);
}

fn claude() -> Claude {
    CLAUDE.read().unwrap().clone().unwrap_or_default()
}

/// `claude -p` with the extra arguments from Settings.
fn command(c: &Claude) -> Command {
    let path = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let mut cmd = Command::new(resolve_bin(&c.bin, &path, &home));
    cmd.arg("-p").args(&c.extra);
    cmd
}

/// The arguments of a prompt run, after `claude -p <extra>`.
pub fn send_args(c: &Claude, choice: &Choice, id: &str, prompt: &str, new: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![if new { "--session-id" } else { "--resume" }.into(), id.into()];
    let model = if choice.model.is_empty() { &c.model } else { &choice.model };
    if !model.is_empty() {
        args.extend(["--model".into(), model.clone()]);
    }
    args.extend(choice.args());
    args.extend(["--output-format", "stream-json", "--verbose", "--include-partial-messages"].map(String::from));
    // A prompt with images goes in on stdin, as a message with image blocks.
    if crate::images::has_images(prompt) {
        args.extend(["--input-format", "stream-json"].map(String::from));
    } else {
        args.push(prompt.into());
    }
    args
}

/// Stops a running prompt, like Ctrl+C in a terminal: SIGINT to claude and everything it
/// started (its own process group), then SIGKILL if it is still there 2 s later.
#[derive(Clone, Default)]
pub struct Stop(Arc<Mutex<Option<u32>>>);

impl Stop {
    pub fn stop(&self) {
        let Some(pid) = *self.0.lock().unwrap() else { return };
        let group = signal_target(pid);
        unsafe { libc::kill(group, libc::SIGINT) };
        let me = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if me.0.lock().unwrap().is_some() {
                unsafe { libc::kill(group, libc::SIGKILL) };
            }
        });
    }
}

/// Where a stop for `pid` goes: its process group when it leads one (runs scope starts do),
/// else (one started in a terminal shares the shell's group) just it.
fn signal_target(pid: u32) -> i32 {
    if unsafe { libc::getpgid(pid as i32) } == pid as i32 { -(pid as i32) } else { pid as i32 }
}

/// Interrupts a `claude` scope doesn't follow (Claude Code in a terminal) like Ctrl+C there:
/// one SIGINT, which stops what it is doing. Nothing harder: that would end the terminal's
/// session too, which Ctrl+C doesn't.
pub fn interrupt(pid: u32) {
    unsafe { libc::kill(signal_target(pid), libc::SIGINT) };
}

/// What a running `claude -p` reports, as it happens.
pub enum RunEvent {
    /// A complete `assistant` / `user` line, in the session file's shape.
    Line(Value),
    /// Text being written in the current reply.
    Delta(String),
    /// A tool call is starting (its name).
    Tool(String),
    Done(Result<(), String>),
}

/// Runs `claude -p` on a thread, streaming its progress: `--resume <id>` for an existing
/// session, `--session-id <id>` to start a new one.
pub fn send(ctx: eframe::egui::Context, id: String, cwd: Option<PathBuf>, prompt: String, new: bool, choice: Choice) -> (Receiver<RunEvent>, Stop) {
    let c = claude();
    let mut cmd = command(&c);
    // Claude can manage scope's sessions through its command, and knows which one it is.
    if let Ok(exe) = std::env::current_exe() {
        cmd.args(["--append-system-prompt", &crate::control::system_prompt(&exe.to_string_lossy())]);
    }
    cmd.env(crate::control::SESSION_VAR, &id);
    cmd.args(send_args(&c, &choice, &id, &prompt, new));
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let input = crate::images::has_images(&prompt).then_some(prompt);
    spawn_stream(ctx, cmd, input)
}

/// Runs `cmd` on a thread in its own process group (so Stop reaches its children too),
/// streaming its events. With `input`, a prompt with images, it is written to claude's stdin.
fn spawn_stream(ctx: eframe::egui::Context, mut cmd: Command, input: Option<String>) -> (Receiver<RunEvent>, Stop) {
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let stop = Stop::default();
    let pid = stop.clone();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let done = stream(&mut cmd, input.as_deref(), &pid, |event| {
            let _ = tx.send(event);
            ctx.request_repaint();
        });
        *pid.0.lock().unwrap() = None;
        let _ = tx.send(RunEvent::Done(done));
        ctx.request_repaint();
    });
    (rx, stop)
}

fn stream(cmd: &mut Command, input: Option<&str>, pid: &Stop, mut emit: impl FnMut(RunEvent)) -> Result<(), String> {
    let line = input.map(crate::images::user_line).transpose()?;
    let mut child = cmd.spawn().map_err(|e| format!("failed to run claude: {e}"))?;
    *pid.0.lock().unwrap() = Some(child.id());
    if let (Some(line), Some(mut stdin)) = (line, child.stdin.take()) {
        // On its own thread: a big image fills the pipe before claude reads it. Closing
        // stdin ends the input, so claude answers and exits.
        std::thread::spawn(move || {
            let _ = writeln!(stdin, "{line}");
        });
    }
    let mut stderr = child.stderr.take();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = stderr.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let mut failure = None;
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        match event(&v) {
            Some(e) => emit(e),
            None if v["type"] == "result" && v["is_error"].as_bool() == Some(true) => {
                failure = Some(v["result"].as_str().unwrap_or("claude reported an error").to_owned());
            }
            None => {}
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let stderr = err_reader.join().unwrap_or_default();
    match failure {
        Some(f) => Err(f),
        None if !status.success() => Err(format!("claude exited with {status}: {}", stderr.trim())),
        None => Ok(()),
    }
}

/// The progress event in one stream-json line, if it carries one.
fn event(v: &Value) -> Option<RunEvent> {
    // Sub-agent steps aren't part of the conversation (the session file marks them sidechain).
    if !v["parent_tool_use_id"].is_null() {
        return None;
    }
    match v["type"].as_str()? {
        "assistant" | "user" => Some(RunEvent::Line(v.clone())),
        "stream_event" => {
            let e = &v["event"];
            match e["type"].as_str()? {
                "content_block_delta" if e["delta"]["type"] == "text_delta" => {
                    Some(RunEvent::Delta(e["delta"]["text"].as_str()?.to_owned()))
                }
                "content_block_start" if e["content_block"]["type"] == "tool_use" => {
                    Some(RunEvent::Tool(e["content_block"]["name"].as_str()?.to_owned()))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Prompt runs (`claude -p` on a session) this scope did not start, as (session id, pid): a
/// scope that has quit leaves its runs going (`claude` finishes and saves its reply without
/// anyone reading it), and another scope or a terminal can start them too.
fn foreign_runs() -> Vec<(String, u32)> {
    let out = Command::new("ps").args(["-axww", "-o", "pid=,ppid=,args="]).stdin(Stdio::null()).output();
    out.map(|o| parse_ps(&String::from_utf8_lossy(&o.stdout), std::process::id())).unwrap_or_default()
}

/// [`foreign_runs`] in `ps -o pid=,ppid=,args=` output; `me` is this scope (its own runs are
/// its children). Side questions (`--fork-session`, nothing saved) don't count.
fn parse_ps(text: &str, me: u32) -> Vec<(String, u32)> {
    let is_uuid = |s: &str| s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    text.lines()
        .filter_map(|line| {
            let mut t = line.split_whitespace();
            let pid: u32 = t.next()?.parse().ok()?;
            let ppid: u32 = t.next()?.parse().ok()?;
            let args: Vec<&str> = t.collect();
            let claude = args.iter().take(2).any(|a| a.contains("claude"));
            let print = args.iter().any(|a| matches!(*a, "-p" | "--print"));
            let side = args.iter().any(|a| matches!(*a, "--fork-session" | "--no-session-persistence"));
            if ppid == me || !claude || !print || side {
                return None;
            }
            let id = args.windows(2).find(|w| matches!(w[0], "--resume" | "-r" | "--session-id"))?[1];
            is_uuid(id).then(|| (id.to_owned(), pid))
        })
        .collect()
}

/// Checks for [`foreign_runs`] every 2 s, sending the list each time it changes.
pub fn watch_runs(ctx: eframe::egui::Context) -> Receiver<Vec<(String, u32)>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut last = None;
        loop {
            let runs = foreign_runs();
            if last.as_ref() != Some(&runs) {
                if tx.send(runs.clone()).is_err() {
                    return;
                }
                ctx.request_repaint();
                last = Some(runs);
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    rx
}

fn alive(pid: u32) -> bool {
    let ok = unsafe { libc::kill(pid as i32, 0) } == 0;
    ok || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Follows a run this scope did not start (see [`foreign_runs`]) through its session file,
/// from the file's current end until process `pid` ends. Claude saves each step whole, so
/// there are no `Delta`s; Stop signals the process like one of scope's own runs.
pub fn follow(ctx: eframe::egui::Context, pid: u32, file: PathBuf) -> (Receiver<RunEvent>, Stop) {
    let stop = Stop(Arc::new(Mutex::new(Some(pid))));
    let me = stop.clone();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut pos = std::fs::metadata(&file).map_or(0, |m| m.len());
        loop {
            // Checked before reading, so the lines written just before it ended are read too.
            let running = alive(pid);
            pos = read_lines(&file, pos, |v| {
                if matches!(v["type"].as_str(), Some("assistant" | "user")) && v["isSidechain"].as_bool() != Some(true) {
                    let _ = tx.send(RunEvent::Line(v));
                    ctx.request_repaint();
                }
            });
            if !running {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        *me.0.lock().unwrap() = None;
        let _ = tx.send(RunEvent::Done(Ok(())));
        ctx.request_repaint();
    });
    (rx, stop)
}

/// The complete lines of `file` after byte `pos`, parsed; returns where the next read starts
/// (a line still being written is read next time).
fn read_lines(file: &Path, pos: u64, mut each: impl FnMut(Value)) -> u64 {
    use std::io::{Seek, SeekFrom};
    let mut buf = Vec::new();
    let read = std::fs::File::open(file).and_then(|mut f| {
        f.seek(SeekFrom::Start(pos))?;
        f.read_to_end(&mut buf)
    });
    if read.is_err() {
        return pos;
    }
    let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return pos };
    for line in buf[..end].split(|&b| b == b'\n') {
        if let Ok(v) = serde_json::from_slice::<Value>(line) {
            each(v);
        }
    }
    pos + end as u64 + 1
}

/// A side question with the session's full context, like Claude Code's /btw: the session is
/// forked and the fork isn't saved, so the original conversation is untouched.
pub fn btw(ctx: eframe::egui::Context, id: String, cwd: Option<PathBuf>, question: String) -> Receiver<Result<String, String>> {
    let mut cmd = command(&claude());
    cmd.args(["--resume", &id, "--fork-session", "--no-session-persistence", &question]);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    run(ctx, cmd)
}

/// Asks a small model for the session goal and a checklist of subgoals (format in goal.rs).
/// Uses --no-session-persistence so the summary itself doesn't become a session.
pub fn summarize_goal(ctx: eframe::egui::Context, words: u32, digest: String) -> Receiver<Result<String, String>> {
    let prompt = format!(
        "Below is a digest of a coding session between a USER and CLAUDE (an AI coding assistant).\n\
         Write the session's overall goal and a checklist of 3 to 7 concrete subgoals.\n\
         Mark a subgoal [x] when CLAUDE reported it done (e.g. it built, committed, verified or \
         shipped it) or the USER moved on to build on it; mark [ ] only if it is still open.\n\
         Keep each subgoal under 12 words. Reply in exactly this format and nothing else:\n\n\
         GOAL: <one or two short sentences, at most {words} words>\n\
         - [x] <completed subgoal>\n\
         - [ ] <open subgoal>\n\n\
         Session digest:\n{digest}"
    );
    ask_haiku(ctx, prompt)
}

/// Asks the helper model for a new session's goal from its first prompt and answer: a summary
/// of the context in at most `words` words, no checklist (that comes with Info → Summarize
/// again, once there is more to go on). `harness` is Settings → Harness → Session goal.
pub fn first_goal(ctx: eframe::egui::Context, harness: &str, words: u32, prompt: &str, answer: &str) -> Receiver<Result<String, String>> {
    let cut = |t: &str, n: usize| t.chars().take(n).collect::<String>();
    let prompt = format!("{}\n\n{}\n\nUSER:\n{}\n\nCLAUDE:\n{}", harness.trim(), first_goal_format(words), cut(prompt, 4000), cut(answer, 4000));
    ask_haiku(ctx, prompt)
}

/// The fixed part of the first goal request, after the harness.
pub fn first_goal_format(words: u32) -> String {
    // Models overshoot a word limit, so this asks for less and the cut (goal::from_summary) is
    // the hard limit.
    let aim = (words * 3 / 4).max(1);
    format!("Reply with the summary only, in at most {words} words: aim for about {aim}, and end on a complete sentence.")
}

/// Asks the helper model for a short name for a new session, from its first prompt and answer.
pub fn name_session(ctx: eframe::egui::Context, prompt: &str, answer: &str) -> Receiver<Result<String, String>> {
    let cut = |t: &str, n: usize| t.chars().take(n).collect::<String>();
    let prompt = format!(
        "Name this coding session in at most 4 words: concise, simple and direct about the thing \
         it addresses (e.g. \"Fix login redirect\", \"Usage limits bar\"). No quotes, no \
         punctuation at the end. Reply with the name only.\n\n\
         USER:\n{}\n\nCLAUDE:\n{}",
        cut(prompt, 2000),
        cut(answer, 2000)
    );
    ask_haiku(ctx, prompt)
}

/// A one-off question to the helper model (Haiku by default), outside any session.
pub fn ask_haiku(ctx: eframe::egui::Context, prompt: String) -> Receiver<Result<String, String>> {
    ask_model(ctx, "", prompt)
}

/// A one-off question to `model` (the helper model when empty), outside any session.
pub fn ask_model(ctx: eframe::egui::Context, model: &str, prompt: String) -> Receiver<Result<String, String>> {
    let c = claude();
    let model = if model.trim().is_empty() { c.helper.clone() } else { model.trim().to_owned() };
    let mut cmd = command(&c);
    cmd.args(["--model", &model, "--no-session-persistence", &prompt]).current_dir(std::env::temp_dir());
    run(ctx, cmd)
}

/// Runs `cmd` on a thread; yields Ok(trimmed stdout) or Err(message), then repaints.
fn run(ctx: eframe::egui::Context, mut cmd: Command) -> Receiver<Result<String, String>> {
    // Without this, `claude -p` waits 3 s for piped stdin before starting.
    cmd.stdin(Stdio::null());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let result = match cmd.output() {
            Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned()),
            Ok(out) => Err(format!("claude exited with {}: {}", out.status, String::from_utf8_lossy(&out.stderr))),
            Err(e) => Err(format!("failed to run claude: {e}")),
        };
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

/// The macOS folder picker, shown in-process (the old `osascript` picker took ~2 s just to
/// come to the front). Blocks until the user picks a folder or cancels (None).
#[cfg(target_os = "macos")]
pub fn pick_folder(start: Option<PathBuf>) -> Option<PathBuf> {
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{MainThreadMarker, NSString, NSURL};
    let mtm = MainThreadMarker::new()?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setCanCreateDirectories(true);
    panel.setMessage(Some(&NSString::from_str("Folder for the new Claude session")));
    panel.setPrompt(Some(&NSString::from_str("Choose")));
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
        panel.setDirectoryURL(Some(&url));
    }
    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    panel.URL().and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string()))
}

#[cfg(not(target_os = "macos"))]
pub fn pick_folder(_start: Option<PathBuf>) -> Option<PathBuf> {
    None
}

/// Moves a file or folder to the Trash (Finder's Put Back can restore it).
#[cfg(target_os = "macos")]
pub fn trash(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| e.localizedDescription().to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn trash(path: &Path) -> Result<(), String> {
    if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }.map_err(|e| e.to_string())
}

/// A system notification (macOS), e.g. when a run ends while scope is in the background.
pub fn notify(title: &str, body: &str) {
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("display notification \"{}\" with title \"{}\"", quote(body), quote(title));
    let _ = Command::new("osascript").args(["-e", &script]).stdin(Stdio::null()).spawn();
}

/// Random UUID v4, as `claude --session-id` requires.
pub fn new_session_id() -> String {
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        // Fallback: time-based bytes; uniqueness is all we need.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        b = t.to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_lines_become_events() {
        let ev = |s: &str| event(&serde_json::from_str(s).unwrap());
        let delta = ev(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}}"#);
        assert!(matches!(delta, Some(RunEvent::Delta(t)) if t == "Hi"));
        let tool = ev(r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","name":"Read","input":{}}}}"#);
        assert!(matches!(tool, Some(RunEvent::Tool(n)) if n == "Read"));
        assert!(matches!(ev(r#"{"type":"assistant","message":{"content":[]}}"#), Some(RunEvent::Line(_))));
        assert!(ev(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#).is_none());
        assert!(ev(r#"{"type":"system","subtype":"init"}"#).is_none());
        assert!(ev(r#"{"type":"assistant","parent_tool_use_id":"toolu_1","message":{"content":[]}}"#).is_none());
    }

    #[test]
    fn a_sessions_choice_becomes_its_flags() {
        assert!(Choice::default().args().is_empty());
        let c = Choice {
            permissions: "plan".into(),
            thinking: "on".into(),
            sources: "user".into(),
            mcp: "none".into(),
            slash: "off".into(),
            agent: "reviewer".into(),
            hooks: "off".into(),
            plugins: "a@m,b@m".into(),
            ..Default::default()
        };
        let args = c.args();
        let has = |k: &str, v: &str| args.windows(2).any(|w| w[0] == k && w[1] == v);
        assert!(has("--permission-mode", "plan") && has("--setting-sources", "user") && has("--agent", "reviewer"));
        assert!(has("--settings", r#"{"alwaysThinkingEnabled":true,"disableAllHooks":true,"enabledPlugins":{"a@m":false,"b@m":false}}"#), "{args:?}");
        assert!(has("--strict-mcp-config", "--mcp-config") && has("--mcp-config", r#"{"mcpServers":{}}"#));
        assert!(args.contains(&"--disable-slash-commands".to_owned()));
    }

    #[test]
    fn send_args_follow_settings() {
        let mut c = Claude::default();
        let none = Choice::default();
        let args = send_args(&c, &none, "id1", "hello", false);
        assert_eq!(args[..2], ["--resume", "id1"]);
        assert!(!args.contains(&"--model".to_owned()));
        assert_eq!(args.last().unwrap(), "hello");
        c.model = "opus".into();
        let args = send_args(&c, &none, "id2", "hi", true);
        assert_eq!(args[..4], ["--session-id", "id2", "--model", "opus"]);
        // A session's own choice wins over Settings, and adds its effort.
        let mine = Choice { model: "fable".into(), effort: "high".into(), ..Default::default() };
        let args = send_args(&c, &mine, "id3", "hi", false);
        assert_eq!(args[..6], ["--resume", "id3", "--model", "fable", "--effort", "high"]);
        let mut s = crate::settings::Settings::default();
        s.extra_args = " --permission-mode  acceptEdits ".into();
        s.claude_path = "".into();
        let c = Claude::from_settings(&s);
        assert_eq!(c.bin, "claude");
        assert_eq!(c.extra, ["--permission-mode", "acceptEdits"]);
    }

    #[test]
    fn foreign_runs_are_prompt_runs_this_scope_did_not_start() {
        let id = "e2b80fff-329c-4904-92c2-9a3c6afe53c5";
        let ps = format!(
            "  101     1 claude -p --resume {id} --output-format stream-json --verbose do it\n\
               102    50 claude -p --resume {id} mine\n\
               103     1 claude -p --resume {id} --fork-session --no-session-persistence side\n\
               104     1 claude --resume {id}\n\
               105     1 node /x/claude-code/cli.js --print --session-id {id} new one\n\
               106     1 vim -p --resume {id}\n\
               107     1 claude -p --resume not-an-id\n"
        );
        assert_eq!(parse_ps(&ps, 50), [(id.to_owned(), 101), (id.to_owned(), 105)]);
    }

    /// A run another scope started: its saved lines arrive, then Done when its process ends.
    #[test]
    fn follow_reads_a_foreign_run_until_its_process_ends() {
        let dir = std::env::temp_dir().join(format!("scope-follow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s.jsonl");
        std::fs::write(&file, "{\"type\":\"user\",\"message\":{\"content\":\"before\"}}\n").unwrap();
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let (rx, stop) = follow(eframe::egui::Context::default(), child.id(), file.clone());
        std::thread::sleep(std::time::Duration::from_millis(300));
        let mut f = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
        use std::io::Write;
        write!(f, "{{\"type\":\"assistant\",\"message\":{{\"content\":[]}}}}\n{{\"type\":\"assistant\",\"isSidechain\":true}}\n{{\"type\":\"user\",\"mess").unwrap();
        let wait = std::time::Duration::from_secs(5);
        let line = rx.recv_timeout(wait).expect("the new line");
        assert!(matches!(line, RunEvent::Line(v) if v["type"] == "assistant"), "only what was written after it started");
        stop.stop();
        let _ = child.wait();
        assert!(matches!(rx.recv_timeout(wait), Ok(RunEvent::Done(Ok(())))), "half a line and sub-agent steps are skipped");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Regression: Ctrl+C must end a run at once, children included, not when it finishes.
    #[test]
    fn stop_ends_a_run_and_its_children_quickly() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30 & echo '{\"type\":\"assistant\",\"message\":{\"content\":[]}}'; wait"]);
        let ctx = eframe::egui::Context::default();
        let (rx, stop) = spawn_stream(ctx, cmd, None);
        let first = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("first line");
        assert!(matches!(first, RunEvent::Line(_)));
        let started = std::time::Instant::now();
        stop.stop();
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(5)).expect("done after stop") {
                RunEvent::Done(r) => {
                    assert!(r.is_err(), "a stopped run reports how it ended");
                    break;
                }
                _ => continue,
            }
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "took {:?}", started.elapsed());
    }

    /// Regression: launched from Finder, PATH lacks ~/.local/bin, so spawning a bare `claude`
    /// failed with "No such file or directory (os error 2)".
    #[test]
    fn merged_path_adds_user_dirs_to_a_finder_path() {
        let p = merged_path("/usr/bin:/bin:/usr/sbin:/sbin", "", "/Users/me");
        let dirs: Vec<&str> = p.split(':').collect();
        assert_eq!(dirs[..4], ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]);
        assert!(dirs.contains(&"/Users/me/.local/bin"));
        assert!(dirs.contains(&"/opt/homebrew/bin"));
        let p = merged_path("/usr/bin:/opt/homebrew/bin", "/custom/bin:/usr/bin", "/Users/me");
        let dirs: Vec<&str> = p.split(':').collect();
        assert_eq!(dirs[..3], ["/usr/bin", "/opt/homebrew/bin", "/custom/bin"]);
        assert_eq!(dirs.iter().filter(|d| **d == "/usr/bin").count(), 1);
        assert_eq!(dirs.iter().filter(|d| **d == "/opt/homebrew/bin").count(), 1);
    }

    #[test]
    fn resolve_bin_finds_claude_outside_the_finder_path() {
        use std::os::unix::fs::PermissionsExt;
        let home = std::env::temp_dir().join(format!("scope-resolve-{}", std::process::id()));
        let bin_dir = home.join(".local/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let claude = bin_dir.join("claude");
        std::fs::write(&claude, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let home_s = home.to_str().unwrap();

        // Not on the bare Finder PATH, found once the user dirs are merged in.
        assert_eq!(resolve_bin("claude", "/usr/bin:/bin", home_s), PathBuf::from("claude"));
        let path = merged_path("/usr/bin:/bin", "", home_s);
        assert_eq!(resolve_bin("claude", &path, home_s), claude);
        // Paths are kept, with ~ expanded.
        assert_eq!(resolve_bin("~/.local/bin/claude", "", home_s), claude);
        assert_eq!(resolve_bin("/opt/x/claude", "", home_s), PathBuf::from("/opt/x/claude"));
        // A non-executable file of that name is skipped.
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(resolve_bin("claude", &path, home_s), PathBuf::from("claude"));
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// The bundled app gets this PATH; the real `claude` must be reachable through it.
    #[test]
    fn login_shell_path_is_nonempty() {
        assert!(!login_shell_path().is_empty());
    }

    #[test]
    fn session_ids_are_uuid_v4_and_distinct() {
        let a = new_session_id();
        assert_eq!(a.len(), 36);
        assert_eq!(a.chars().filter(|&c| c == '-').count(), 4);
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
        assert_ne!(a, new_session_id());
    }
}
