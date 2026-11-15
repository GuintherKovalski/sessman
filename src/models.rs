//! "Look for model updates" in the model dropdowns: asks Claude (`claude -p` with the helper
//! model, allowed to read the web) which Claude models there are now, so a model released
//! after scope was built shows up without a new scope. Runs on the developer's Claude Code
//! login; no API key needed.

use std::sync::mpsc::{channel, Receiver};

/// What Claude is asked; the reply is read by [`ids`].
pub const PROMPT: &str = "List the current Claude model IDs from Anthropic's models overview page \
(https://platform.claude.com/docs/en/about-claude/models/overview); fetch it, and search the web if it fails. \
For each model family (opus, sonnet, haiku, fable, ...) give the newest model ID, as used with Claude Code's --model \
(for example claude-opus-5-5), without date suffixes. Leave out models only some organizations can use (like Mythos). Reply with the IDs only, one per line, nothing else.";

/// Starts the lookup; the answer is the model ids Claude listed.
pub fn check(ctx: eframe::egui::Context) -> Receiver<Result<Vec<String>, String>> {
    let reply = crate::runner::ask_with_web(ctx.clone(), PROMPT.to_owned());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let result = match reply.recv() {
            Ok(Ok(text)) => match ids(&text) {
                found if found.is_empty() => Err("Claude's reply had no model names".to_owned()),
                found => Ok(found),
            },
            Ok(Err(e)) => Err(e),
            Err(_) => Err("the lookup stopped".to_owned()),
        };
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

/// The model ids in a reply: words like `claude-fable-5-2`, without dated snapshots
/// (`claude-sonnet-4-5-20250929`); the dropdowns offer each family's newest by its plain name.
fn ids(reply: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in reply.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        let id = word.trim_matches('-').to_ascii_lowercase();
        let dated = id.split('-').any(|p| p.len() >= 8 && p.bytes().all(|b| b.is_ascii_digit()));
        if id.starts_with("claude-") && !dated && !out.contains(&id) {
            out.push(id);
        }
    }
    out
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
    fn a_newer_model_in_the_reply_replaces_its_family() {
        let reply = "claude-fable-5-2\n- `claude-opus-5-5`\nclaude-sonnet-4-5-20250929\nclaude-fable-5-1\nClaude-Mythos-5-2.";
        let found = ids(reply);
        assert_eq!(found, ["claude-fable-5-2", "claude-opus-5-5", "claude-fable-5-1", "claude-mythos-5-2"]);
        let mut known: Vec<String> = ["claude-opus-5-5", "claude-fable-5-1"].map(String::from).to_vec();
        let new = apply(&mut known, &found);
        assert_eq!(known, ["claude-opus-5-5", "claude-fable-5-2", "claude-mythos-5-2"]);
        assert_eq!(new, ["claude-fable-5-2", "claude-mythos-5-2"]);
        assert!(apply(&mut known, &found).is_empty(), "a second look finds nothing new");
    }

    #[test]
    fn no_model_names_no_models() {
        assert!(ids("Sorry, I could not fetch the page.").is_empty());
    }
}
