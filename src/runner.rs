use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};

/// Runs `claude -p --resume <id> <prompt>` on a thread. The receiver yields Ok(stdout) or Err(message).
pub fn send(ctx: eframe::egui::Context, id: String, cwd: Option<PathBuf>, prompt: String) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut cmd = Command::new("claude");
        cmd.args(["-p", "--resume", &id, &prompt]);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let result = match cmd.output() {
            Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            Ok(out) => Err(format!("claude exited with {}: {}", out.status, String::from_utf8_lossy(&out.stderr))),
            Err(e) => Err(format!("failed to run claude: {e}")),
        };
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}
