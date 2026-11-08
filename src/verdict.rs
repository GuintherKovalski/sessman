//! Whether a session's work looks finished. After a session's run ends (and, from time to time,
//! for workspace sessions with activity it hasn't judged), the helper model reads the end of its
//! conversation and tags it: done, waiting for the developer, or blocked. The tag holds for the
//! activity it read; anything newer in the session makes it stale until it is checked again.
use crate::{overview, runner, session};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

/// "done", "waiting" or "blocked".
pub const STATES: [&str; 3] = ["done", "waiting", "blocked"];

/// Characters of conversation the model reads (mostly its end).
const DIGEST: usize = 4000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Verdict {
    pub state: String,
    /// One short sentence: what was achieved, or what is needed.
    pub why: String,
    /// The session's newest change (Unix seconds) when it was read.
    pub at: u64,
}

impl Verdict {
    /// How the bar and `scope list` show it.
    pub fn label(&self) -> &'static str {
        match self.state.as_str() {
            "done" => "✔ done",
            "blocked" => "⚠ blocked",
            _ => "✋ needs you",
        }
    }
}

/// Session id → verdict, saved as JSON (shared by every scope on the config folder:
/// `shared::Shared`).
#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(transparent)]
pub struct Verdicts(HashMap<String, Verdict>);

impl Verdicts {
    pub fn path() -> PathBuf {
        crate::settings::config_dir().join("verdicts.json")
    }

    pub fn set(&mut self, id: &str, v: Verdict) {
        self.0.insert(id.to_owned(), v);
    }

    /// The verdict on the session as it is now (`modified`), if it has one.
    pub fn current(&self, id: &str, modified: u64) -> Option<&Verdict> {
        self.0.get(id).filter(|v| v.at >= modified)
    }

    /// Whether the session has activity no verdict has read.
    pub fn stale(&self, id: &str, modified: u64) -> bool {
        self.current(id, modified).is_none()
    }
}

/// A session waiting to be judged, newest change first: (id, modified).
/// Picks those not running, quiet for `quiet` seconds, changed within `window` seconds, with
/// activity no verdict (and no failed check: `failed`, id → modified) has read.
pub fn candidates(sessions: &[(String, u64, bool)], verdicts: &Verdicts, failed: &HashMap<String, u64>, now: u64, quiet: u64, window: u64) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = sessions
        .iter()
        .filter(|(id, modified, running)| {
            !running
                && now.saturating_sub(*modified) >= quiet
                && now.saturating_sub(*modified) <= window
                && verdicts.stale(id, *modified)
                && failed.get(id).is_none_or(|&f| f < *modified)
        })
        .map(|(id, m, _)| (id.clone(), *m))
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.1));
    out
}

/// What each state means, shared by this check and the orchestration report, so a session
/// reads the same in the bar and on its tile. Red (blocked) is for what takes real attention;
/// yellow (waiting) for a decision, answer or quick look.
pub const MEANING: &str = "\
- \"done\": what the user asked for is complete (built, committed, merged, answered, \
delivered). A closing offer of optional extras or cleanup (\"want me to stop the test \
instance?\") still counts as done.\n\
- \"waiting\": nothing went wrong and the next step is a decision: the user has to answer a \
question, choose between options, approve a step (ship, delete, merge), or glance at a result; \
or Claude paused on something still running (e.g. \"watching CI\") with nothing failed.\n\
- \"blocked\": it needs more of the user's attention than a decision: something failed that \
Claude could not get past (errors, failing tests it could not fix, a broken tool), it lacks \
access, credentials or setup only the user can provide, it is confused or going in circles, or \
the user has to investigate, fix or redo something before work can go on.\n\
If something failed and Claude cannot continue, it is \"blocked\" even when it ends by asking \
the user to do something; if nothing failed and only a choice or answer is needed, it is \
\"waiting\".\n";

pub fn request(title: &str, digest: &str) -> String {
    format!(
        "Has this Claude Code session finished its work? Below is the end of a conversation \
         between a USER (a developer) and CLAUDE (an AI coding assistant). Claude is not running \
         now: it ended its turn. Judge from its last messages:\n\
         {MEANING}\
         Reply with JSON only: {{\"state\": \"done|waiting|blocked\", \"why\": \"one sentence under \
         15 words: what was achieved, or what is needed\"}}\n\n\
         SESSION: {title}\n\n{digest}"
    )
}

/// The model's reply as (state, why), tolerating text or code fences around the JSON.
pub fn parse(reply: &str) -> Option<(String, String)> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let v: serde_json::Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let state = v["state"].as_str()?.trim().to_lowercase();
    if !STATES.contains(&state.as_str()) {
        return None;
    }
    let why = v["why"].as_str().unwrap_or_default().trim().to_owned();
    Some((state, why))
}

/// Judges the session in `path` on a thread with the helper model.
pub fn check(ctx: eframe::egui::Context, title: String, path: PathBuf) -> Receiver<Result<Verdict, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let at = std::fs::metadata(&path).and_then(|m| m.modified()).map(overview::secs).unwrap_or(0);
        let result = session::load(&path).map_err(|e| e.to_string()).and_then(|s| {
            let digest = session::digest(&s.messages, 600, DIGEST);
            runner::ask_haiku(ctx.clone(), request(&title, &digest))
                .recv()
                .unwrap_or_else(|_| Err("the model stopped unexpectedly".into()))
                .and_then(|reply| parse(&reply).ok_or_else(|| format!("unexpected reply: {reply}")))
                .map(|(state, why)| Verdict { state, why, at })
        });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_takes_known_states_around_noise() {
        assert_eq!(parse("```json\n{\"state\": \"Done\", \"why\": \"Merged to main.\"}\n```"), Some(("done".into(), "Merged to main.".into())));
        assert_eq!(parse("{\"state\": \"blocked\"}"), Some(("blocked".into(), String::new())));
        assert_eq!(parse("{\"state\": \"working\", \"why\": \"x\"}"), None);
        assert_eq!(parse("no json"), None);
    }

    #[test]
    fn a_verdict_holds_until_newer_activity() {
        let mut v = Verdicts::default();
        v.set("a", Verdict { state: "done".into(), why: String::new(), at: 100 });
        assert!(v.current("a", 100).is_some());
        assert!(v.current("a", 101).is_none());
        assert!(v.stale("b", 5));
    }

    #[test]
    fn candidates_are_quiet_recent_unjudged_and_not_running() {
        let mut v = Verdicts::default();
        v.set("judged", Verdict { state: "done".into(), why: String::new(), at: 900 });
        let failed = HashMap::from([("failed".to_owned(), 900)]);
        let s = |id: &str, m: u64, r: bool| (id.to_owned(), m, r);
        let sessions = vec![
            s("old", 100, false),
            s("judged", 900, false),
            s("newer", 920, false),
            s("running", 900, true),
            s("busy", 990, false),
            s("failed", 900, false),
            s("new", 950, false),
        ];
        let got = candidates(&sessions, &v, &failed, 1000, 30, 500);
        assert_eq!(got, vec![("new".to_owned(), 950), ("newer".to_owned(), 920)]);
    }
}
