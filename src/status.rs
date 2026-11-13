//! Usage limits and exact context size are only given by Claude Code to a status-line command
//! (JSON on stdin). `scope statusline` is that command: it saves the JSON for the app to read
//! and prints a short status line. https://code.claude.com/docs/en/statusline.md
//!
//! The status line only runs in interactive sessions, so the limits it saw go stale while scope
//! runs `claude -p`. [`watch_usage`] asks the endpoint `/usage` reads instead.
use serde_json::{Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

pub fn dir() -> PathBuf {
    crate::settings::config_dir().join("status")
}

fn settings_path() -> PathBuf {
    crate::platform::claude_dir().join("settings.json")
}

/// Entry point for `scope statusline`, run by Claude Code.
pub fn run() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let Ok(v) = serde_json::from_str::<Value>(&input) else { return };
    let dir = dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Some(id) = v["session_id"].as_str().filter(|id| !id.contains('/')) {
        let _ = std::fs::write(dir.join(format!("{id}.json")), &input);
    }
    // Limits are per account, so the latest payload from any session is the freshest.
    if v.get("rate_limits").is_some_and(|r| r.is_object()) {
        let _ = std::fs::write(dir.join("latest.json"), &input);
    }
    println!("{}", line(&v));
}

/// "Opus · ctx 18% · 5h 23% · 7d 41%"
pub fn line(v: &Value) -> String {
    let mut parts = vec![v["model"]["display_name"].as_str().unwrap_or("Claude").to_owned()];
    if let Some(p) = v["context_window"]["used_percentage"].as_f64() {
        parts.push(format!("ctx {p:.0}%"));
    }
    for w in limits(v) {
        parts.push(format!("{} {:.0}%", w.short, w.used));
    }
    parts.join(" · ")
}

#[derive(Clone)]
pub struct Window {
    pub label: String,
    pub short: &'static str,
    pub used: f64,
    pub resets_at: Option<u64>,
}

pub fn limits(v: &Value) -> Vec<Window> {
    [("five_hour", "Session (5-hour)", "5h"), ("seven_day", "Weekly", "7d"), ("spend_limit", "Spend limit", "spend")]
        .into_iter()
        .filter_map(|(key, label, short)| {
            let w = &v["rate_limits"][key];
            Some(Window { label: label.to_owned(), short, used: w["used_percentage"].as_f64()?, resets_at: w["resets_at"].as_u64() })
        })
        .collect()
}

/// The account's limits as `/usage` shows them, from the reply of its endpoint.
pub fn usage_limits(v: &Value) -> Vec<Window> {
    let mut out: Vec<Window> = [("five_hour", "Session (5-hour)", "5h"), ("seven_day", "Weekly", "7d")]
        .into_iter()
        .filter_map(|(key, label, short)| {
            let w = &v[key];
            Some(Window { label: label.to_owned(), short, used: w["utilization"].as_f64()?, resets_at: w["resets_at"].as_str().and_then(unix_time) })
        })
        .collect();
    // Weekly limits of one model ("Weekly (Opus)"), under whichever name the reply uses.
    let scoped = v["limits"].as_array().into_iter().flatten().filter(|l| l["kind"] == "weekly_scoped");
    let mut models: Vec<(String, &Value)> = scoped.filter_map(|l| Some((l["scope"]["model"]["display_name"].as_str()?.to_owned(), l))).collect();
    if models.is_empty() {
        for (key, name) in [("seven_day_opus", "Opus"), ("seven_day_sonnet", "Sonnet")] {
            if v[key].is_object() {
                models.push((name.to_owned(), &v[key]));
            }
        }
    }
    for (name, w) in models {
        let Some(used) = w["percent"].as_f64().or_else(|| w["utilization"].as_f64()) else { continue };
        out.push(Window { label: format!("Weekly ({name})"), short: "7d", used, resets_at: w["resets_at"].as_str().and_then(unix_time) });
    }
    out
}

/// "2026-10-21T16:39:59.887841+00:00" → Unix seconds (UTC offsets are honored).
fn unix_time(s: &str) -> Option<u64> {
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, sec) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    // Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    let tz = &s[19..];
    let tz = tz.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match tz.as_bytes().first() {
        Some(b'+' | b'-') => {
            let m = tz.get(1..3)?.parse::<i64>().ok()? * 3600 + tz.get(4..6)?.parse::<i64>().ok()? * 60;
            if tz.starts_with('-') { -m } else { m }
        }
        _ => 0,
    };
    u64::try_from(days * 86400 + h * 3600 + mi * 60 + sec - offset).ok()
}

/// Claude Code's OAuth token: in the login keychain on macOS, else (Windows, Linux) in
/// ~/.claude/.credentials.json.
fn access_token() -> Result<String, String> {
    let keychain = if !cfg!(target_os = "macos") {
        None
    } else {
        std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let text = keychain
        .or_else(|| std::fs::read_to_string(crate::platform::claude_dir().join(".credentials.json")).ok())
        .ok_or("not signed in to Claude Code with a Claude account")?;
    let v: Value = serde_json::from_str(text.trim()).map_err(|_| "can't read Claude Code's credentials")?;
    v["claudeAiOauth"]["accessToken"].as_str().map(str::to_owned).ok_or_else(|| "not signed in to Claude Code with a Claude account".to_owned())
}

/// Asks the usage endpoint once (with curl, so the token stays out of the process list).
fn fetch_usage() -> Result<Vec<Window>, String> {
    let token = access_token()?;
    let mut curl = std::process::Command::new("curl");
    crate::platform::hide_window(&mut curl);
    let mut child = curl
        .args(["-sS", "--max-time", "15", "-K", "-", "https://api.anthropic.com/api/oauth/usage"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("can't start curl: {e}"))?;
    let config = format!(
        "header = \"Authorization: Bearer {token}\"\nheader = \"anthropic-beta: oauth-2025-04-20\"\nheader = \"User-Agent: scope\"\n"
    );
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(config.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "unexpected reply from the usage endpoint".to_owned())?;
    if let Some(msg) = v["error"]["message"].as_str() {
        return Err(msg.to_owned());
    }
    match usage_limits(&v) {
        l if l.is_empty() => Err("no limits in the usage endpoint's reply".to_owned()),
        l => Ok(l),
    }
}

/// Asks for the account's limits every 10 s while `busy` (Claude is working somewhere), else
/// every 10 minutes; sends each answer.
pub fn watch_usage(ctx: eframe::egui::Context, busy: Arc<AtomicBool>) -> Receiver<Result<Vec<Window>, String>> {
    let (tx, rx) = channel();
    // Tests drive the app against a fake claude; they must not reach the real account.
    if cfg!(test) {
        return rx;
    }
    std::thread::spawn(move || {
        loop {
            if tx.send(fetch_usage()).is_err() {
                return;
            }
            ctx.request_repaint();
            let asked = Instant::now();
            while asked.elapsed() < if busy.load(Ordering::Relaxed) { Duration::from_secs(10) } else { Duration::from_secs(600) } {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
    rx
}

/// A saved payload and when it was written.
pub fn load(path: &Path) -> Option<(Value, SystemTime)> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some((serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?, modified))
}

pub enum Setup {
    Enabled,
    NotSet,
    /// The user already has their own status line; we won't replace it.
    Other(String),
}

pub fn setup() -> Setup {
    let settings = load(&settings_path()).map(|(v, _)| v).unwrap_or_default();
    match settings["statusLine"]["command"].as_str() {
        Some(c) if c.contains("scope") && c.ends_with("statusline") => Setup::Enabled,
        // From before the app was renamed scope: that binary saves to the old folder.
        Some(c) if c.contains("sessman") && c.ends_with("statusline") => Setup::NotSet,
        Some(c) => Setup::Other(c.to_owned()),
        None => Setup::NotSet,
    }
}

/// Points Claude Code's status line at this binary. Keeps every other setting as is.
pub fn enable() -> Result<(), String> {
    let path = settings_path();
    let mut settings: Value = match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("can't parse {}: {e}", path.display()))?,
        Err(_) => json!({}),
    };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Windows: Claude Code may run it with Git Bash or cmd, which share double quotes and `/`.
    let command = if cfg!(windows) { format!("\"{}\" statusline", crate::platform::shell_path(&exe)) } else { format!("'{}' statusline", exe.display()) };
    settings["statusLine"] = json!({ "type": "command", "command": command });
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_limits_and_formats_line() {
        let v: Value = serde_json::from_str(
            r#"{"model":{"display_name":"Opus"},"context_window":{"used_percentage":18.2},
                "rate_limits":{"five_hour":{"used_percentage":23.5,"resets_at":1738425600},
                               "seven_day":{"used_percentage":41.2,"resets_at":1738857600}}}"#,
        )
        .unwrap();
        let l = limits(&v);
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].label.as_str(), l[0].resets_at), ("Session (5-hour)", Some(1738425600)));
        assert_eq!(line(&v), "Opus · ctx 18% · 5h 24% · 7d 41%");
        assert_eq!(line(&json!({})), "Claude");
    }

    #[test]
    fn reads_usage_endpoint() {
        let v: Value = serde_json::from_str(
            r#"{"five_hour":{"utilization":53.0,"resets_at":"2026-10-21T16:39:59.887841+00:00"},
                "seven_day":{"utilization":31.0,"resets_at":"2026-10-22T09:59:59.887862+00:00"},
                "seven_day_opus":null,
                "limits":[{"kind":"session","percent":53},
                          {"kind":"weekly_scoped","percent":4,"resets_at":"2026-10-22T10:00:00+00:00","scope":{"model":{"display_name":"Fable"}}}]}"#,
        )
        .unwrap();
        let l = usage_limits(&v);
        let got: Vec<_> = l.iter().map(|w| (w.label.as_str(), w.used, w.resets_at)).collect();
        assert_eq!(
            got,
            [("Session (5-hour)", 53.0, Some(1792600799)), ("Weekly", 31.0, Some(1792663199)), ("Weekly (Fable)", 4.0, Some(1792663200))]
        );
        assert_eq!(unix_time("1970-01-01T01:00:00+01:00"), Some(0));
        assert_eq!(unix_time("2000-03-01T00:00:00Z"), Some(951868800));
    }
}
