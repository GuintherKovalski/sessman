//! Usage limits and exact context size are only given by Claude Code to a status-line command
//! (JSON on stdin). `scope statusline` is that command: it saves the JSON for the app to read
//! and prints a short status line. https://code.claude.com/docs/en/statusline.md
use serde_json::{Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

pub fn dir() -> PathBuf {
    crate::settings::config_dir().join("status")
}

fn settings_path() -> PathBuf {
    home().join(".claude/settings.json")
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

pub struct Window {
    pub label: &'static str,
    pub short: &'static str,
    pub used: f64,
    pub resets_at: Option<u64>,
}

pub fn limits(v: &Value) -> Vec<Window> {
    [("five_hour", "5-hour session", "5h"), ("seven_day", "Weekly", "7d"), ("spend_limit", "Spend limit", "spend")]
        .into_iter()
        .filter_map(|(key, label, short)| {
            let w = &v["rate_limits"][key];
            Some(Window { label, short, used: w["used_percentage"].as_f64()?, resets_at: w["resets_at"].as_u64() })
        })
        .collect()
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
    let command = format!("'{}' statusline", exe.display());
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
        assert_eq!((l[0].label, l[0].resets_at), ("5-hour session", Some(1738425600)));
        assert_eq!(line(&v), "Opus · ctx 18% · 5h 24% · 7d 41%");
        assert_eq!(line(&json!({})), "Claude");
    }
}
