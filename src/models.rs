//! "Look for model updates" in the model dropdowns: asks the Claude API (`GET /v1/models`)
//! which models the account can use, so a model released after scope was built shows up
//! without a new scope. Claude Code's own login can't be used for this; the API wants an API
//! key (`ANTHROPIC_API_KEY`, here or in the login shell) or an `ant auth login` profile.

use serde_json::Value;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

const URL: &str = "https://api.anthropic.com/v1/models";

/// How a request is signed: an API key, or an OAuth access token (from `ant`).
enum Credential {
    Key(String),
    Token(String),
}

/// Starts the lookup on a thread; the answer is the ids of every model the API lists.
pub fn check(ctx: eframe::egui::Context) -> Receiver<Result<Vec<String>, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(credential().and_then(|c| list(&c)));
        ctx.request_repaint();
    });
    rx
}

fn credential() -> Result<Credential, String> {
    let key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty()).or_else(login_shell_key);
    if let Some(k) = key {
        return Ok(Credential::Key(k.trim().to_owned()));
    }
    let ant = Command::new("ant").args(["auth", "print-credentials", "--access-token"]).stderr(Stdio::null()).output();
    if let Some(out) = ant.ok().filter(|o| o.status.success()) {
        let token = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !token.is_empty() {
            return Ok(Credential::Token(token));
        }
    }
    Err("No Claude API key: set ANTHROPIC_API_KEY (or run `ant auth login`), then restart scope".into())
}

/// `ANTHROPIC_API_KEY` as the login shell sets it (an app opened from the Dock doesn't get the
/// shell's variables).
fn login_shell_key() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let out = Command::new(shell)
        .args(["-ilc", "printf '\\n__SCOPE_KEY__%s\\n' \"$ANTHROPIC_API_KEY\""])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines().find_map(|l| l.strip_prefix("__SCOPE_KEY__")).map(str::trim).filter(|k| !k.is_empty()).map(str::to_owned)
}

/// Every page of the model list.
fn list(cred: &Credential) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let url = match &after {
            Some(a) => format!("{URL}?limit=1000&after_id={a}"),
            None => format!("{URL}?limit=1000"),
        };
        let page = get(&url, cred)?;
        if let Some(e) = page["error"]["message"].as_str() {
            return Err(format!("Claude API: {e}"));
        }
        ids.extend(page_ids(&page));
        match page["last_id"].as_str() {
            Some(last) if page["has_more"].as_bool() == Some(true) => after = Some(last.to_owned()),
            _ => return Ok(ids),
        }
    }
}

/// One request through curl. The credential goes in on stdin (`-K -`), never in the
/// arguments, where other processes could read it.
fn get(url: &str, cred: &Credential) -> Result<Value, String> {
    let mut config = format!("url = \"{url}\"\nheader = \"anthropic-version: 2023-06-01\"\n");
    match cred {
        Credential::Key(k) => config += &format!("header = \"x-api-key: {k}\"\n"),
        Credential::Token(t) => {
            config += &format!("header = \"Authorization: Bearer {t}\"\nheader = \"anthropic-beta: oauth-2025-04-20\"\n");
        }
    }
    let mut child = Command::new("curl")
        .args(["-sS", "--max-time", "20", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run curl: {e}"))?;
    child.stdin.take().unwrap().write_all(config.as_bytes()).map_err(|e| e.to_string())?;
    let mut body = String::new();
    child.stdout.take().unwrap().read_to_string(&mut body).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("could not reach the Claude API: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    serde_json::from_str(&body).map_err(|_| "the Claude API sent an answer scope can't read".into())
}

/// The ids on one page, without dated snapshots (`claude-sonnet-4-5-20250929`): the
/// dropdowns offer each family's newest model by its plain name.
fn page_ids(page: &Value) -> Vec<String> {
    page["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str())
        .filter(|id| !id.split('-').any(|p| p.len() >= 8 && p.bytes().all(|b| b.is_ascii_digit())))
        .map(str::to_owned)
        .collect()
}

/// Learns `found` into `known` (see [`crate::newinfo::learn`]); the names that are new.
pub fn apply(known: &mut Vec<String>, found: &[String]) -> Vec<String> {
    let before = known.clone();
    for id in found {
        crate::newinfo::learn(known, id);
    }
    known.iter().filter(|k| !before.contains(k)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_model_from_the_api_replaces_its_family() {
        let page: Value = serde_json::from_str(
            r#"{"data":[{"id":"claude-fable-5-2"},{"id":"claude-opus-5-5"},{"id":"claude-sonnet-4-5-20250929"},
                        {"id":"claude-fable-5-1"},{"id":"claude-mythos-5-2"}],"has_more":false}"#,
        )
        .unwrap();
        let found = page_ids(&page);
        assert!(!found.iter().any(|f| f.contains("2025")), "dated snapshots are left out");
        let mut known: Vec<String> = ["claude-opus-5-5", "claude-fable-5-1"].map(String::from).to_vec();
        let new = apply(&mut known, &found);
        assert_eq!(known, ["claude-opus-5-5", "claude-fable-5-2", "claude-mythos-5-2"]);
        assert_eq!(new, ["claude-fable-5-2", "claude-mythos-5-2"]);
        assert!(apply(&mut known, &found).is_empty(), "a second look finds nothing new");
    }
}
