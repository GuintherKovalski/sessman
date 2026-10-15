use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

/// Runs `claude -p` on a thread: `--resume <id>` for an existing session, `--session-id <id>`
/// to start a new one. The receiver yields Ok(stdout) or Err(message).
pub fn send(
    ctx: eframe::egui::Context,
    id: String,
    cwd: Option<PathBuf>,
    prompt: String,
    new: bool,
) -> Receiver<Result<String, String>> {
    let mut cmd = Command::new("claude");
    cmd.args(["-p", if new { "--session-id" } else { "--resume" }, &id, &prompt]);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    run(ctx, cmd)
}

/// A side question with the session's full context, like Claude Code's /btw: the session is
/// forked and the fork isn't saved, so the original conversation is untouched.
pub fn btw(ctx: eframe::egui::Context, id: String, cwd: Option<PathBuf>, question: String) -> Receiver<Result<String, String>> {
    let mut cmd = Command::new("claude");
    cmd.args(["-p", "--resume", &id, "--fork-session", "--no-session-persistence", &question]);
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
    let mut cmd = Command::new("claude");
    cmd.args(["-p", "--model", "haiku", "--no-session-persistence", &prompt]).current_dir(std::env::temp_dir());
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

/// Opens the macOS folder picker on a thread. Yields None if the user cancels.
pub fn pick_folder(ctx: eframe::egui::Context, start: Option<PathBuf>) -> Receiver<Option<PathBuf>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut script = String::from("POSIX path of (choose folder with prompt \"Folder for the new Claude session\"");
        if let Some(dir) = start.filter(|d| d.is_dir()) {
            let dir = dir.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
            script.push_str(&format!(" default location POSIX file \"{dir}\""));
        }
        script.push(')');
        let out = Command::new("osascript").args(["-e", "tell me to activate", "-e", &script]).output();
        let dir = out
            .ok()
            .filter(|o| o.status.success())
            .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
        let _ = tx.send(dir);
        ctx.request_repaint();
    });
    rx
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
    fn session_ids_are_uuid_v4_and_distinct() {
        let a = new_session_id();
        assert_eq!(a.len(), 36);
        assert_eq!(a.chars().filter(|&c| c == '-').count(), 4);
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
        assert_ne!(a, new_session_id());
    }
}
