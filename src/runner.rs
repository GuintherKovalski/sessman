use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex, RwLock};

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

static CLAUDE: RwLock<Option<Claude>> = RwLock::new(None);

pub fn configure(c: Claude) {
    *CLAUDE.write().unwrap() = Some(c);
}

fn claude() -> Claude {
    CLAUDE.read().unwrap().clone().unwrap_or_default()
}

/// `claude -p` with the extra arguments from Settings.
fn command(c: &Claude) -> Command {
    let mut cmd = Command::new(&c.bin);
    cmd.arg("-p").args(&c.extra);
    cmd
}

/// The arguments of a prompt run, after `claude -p <extra>`.
pub fn send_args(c: &Claude, id: &str, prompt: &str, new: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![if new { "--session-id" } else { "--resume" }.into(), id.into()];
    if !c.model.is_empty() {
        args.extend(["--model".into(), c.model.clone()]);
    }
    args.extend(["--output-format", "stream-json", "--verbose", "--include-partial-messages"].map(String::from));
    args.push(prompt.into());
    args
}

/// Stops a running prompt, like Ctrl+C in a terminal: SIGINT to claude and everything it
/// started (its own process group), then SIGKILL if it is still there 2 s later.
#[derive(Clone, Default)]
pub struct Stop(Arc<Mutex<Option<u32>>>);

impl Stop {
    pub fn stop(&self) {
        let Some(pid) = *self.0.lock().unwrap() else { return };
        let group = -(pid as i32);
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
pub fn send(ctx: eframe::egui::Context, id: String, cwd: Option<PathBuf>, prompt: String, new: bool) -> (Receiver<RunEvent>, Stop) {
    let c = claude();
    let mut cmd = command(&c);
    cmd.args(send_args(&c, &id, &prompt, new));
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    spawn_stream(ctx, cmd)
}

/// Runs `cmd` on a thread in its own process group (so Stop reaches its children too),
/// streaming its events.
fn spawn_stream(ctx: eframe::egui::Context, mut cmd: Command) -> (Receiver<RunEvent>, Stop) {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let stop = Stop::default();
    let pid = stop.clone();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let done = stream(&mut cmd, &pid, |event| {
            let _ = tx.send(event);
            ctx.request_repaint();
        });
        *pid.0.lock().unwrap() = None;
        let _ = tx.send(RunEvent::Done(done));
        ctx.request_repaint();
    });
    (rx, stop)
}

fn stream(cmd: &mut Command, pid: &Stop, mut emit: impl FnMut(RunEvent)) -> Result<(), String> {
    let mut child = cmd.spawn().map_err(|e| format!("failed to run claude: {e}"))?;
    *pid.0.lock().unwrap() = Some(child.id());
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
pub fn summarize_goal(ctx: eframe::egui::Context, digest: String) -> Receiver<Result<String, String>> {
    let prompt = format!(
        "Below is a digest of a coding session between a USER and CLAUDE (an AI coding assistant).\n\
         Write the session's overall goal and a checklist of 3 to 7 concrete subgoals.\n\
         Mark a subgoal [x] when CLAUDE reported it done (e.g. it built, committed, verified or \
         shipped it) or the USER moved on to build on it; mark [ ] only if it is still open.\n\
         Keep each subgoal under 12 words. Reply in exactly this format and nothing else:\n\n\
         GOAL: <one or two short sentences>\n\
         - [x] <completed subgoal>\n\
         - [ ] <open subgoal>\n\n\
         Session digest:\n{digest}"
    );
    ask_haiku(ctx, prompt)
}

/// A one-off question to the helper model (Haiku by default), outside any session.
pub fn ask_haiku(ctx: eframe::egui::Context, prompt: String) -> Receiver<Result<String, String>> {
    let c = claude();
    let mut cmd = command(&c);
    cmd.args(["--model", &c.helper, "--no-session-persistence", &prompt]).current_dir(std::env::temp_dir());
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

/// A system notification (macOS), e.g. when a run ends while sessman is in the background.
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
    fn send_args_follow_settings() {
        let mut c = Claude::default();
        let args = send_args(&c, "id1", "hello", false);
        assert_eq!(args[..2], ["--resume", "id1"]);
        assert!(!args.contains(&"--model".to_owned()));
        assert_eq!(args.last().unwrap(), "hello");
        c.model = "opus".into();
        let args = send_args(&c, "id2", "hi", true);
        assert_eq!(args[..4], ["--session-id", "id2", "--model", "opus"]);
        let mut s = crate::settings::Settings::default();
        s.extra_args = " --permission-mode  acceptEdits ".into();
        s.claude_path = "".into();
        let c = Claude::from_settings(&s);
        assert_eq!(c.bin, "claude");
        assert_eq!(c.extra, ["--permission-mode", "acceptEdits"]);
    }

    /// Regression: Ctrl+C must end a run at once, children included, not when it finishes.
    #[test]
    fn stop_ends_a_run_and_its_children_quickly() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30 & echo '{\"type\":\"assistant\",\"message\":{\"content\":[]}}'; wait"]);
        let ctx = eframe::egui::Context::default();
        let (rx, stop) = spawn_stream(ctx, cmd);
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
