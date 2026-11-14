//! What scope restores after a restart: the open workspace and session, each session's
//! file tabs, draft and queue, and the layout. Window size and panel widths are saved by
//! eframe (`window.ron` in the same folder).
use crate::queue::Queue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The main area of one session, kept while other sessions are open.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct View {
    pub files: Vec<OpenFile>,
    /// Active tab: None = the conversation, Some(i) = files[i].
    pub active: Option<usize>,
    pub draft: String,
    pub changes_filter: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenFile {
    pub path: String,
    pub rendered: bool,
}

impl View {
    pub fn is_empty(&self) -> bool {
        *self == View::default()
    }
}

/// Views of the sessions not on screen, by session key.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Views(pub HashMap<String, View>);

impl Views {
    /// Switches the main area from one session to another: stores `current` under `from`
    /// and returns what `to` had (empty if it never had anything).
    pub fn switch(&mut self, from: Option<&str>, to: Option<&str>, current: View) -> View {
        if from == to {
            return current;
        }
        if let Some(f) = from {
            if current.is_empty() {
                self.0.remove(f);
            } else {
                self.0.insert(f.to_owned(), current);
            }
        }
        to.and_then(|t| self.0.remove(t)).unwrap_or_default()
    }
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(default)]
pub struct State {
    /// Workspace name; None = All sessions.
    pub tab: Option<String>,
    /// Session id open in the main area.
    pub open: Option<String>,
    /// Every session's view, the open one included.
    pub views: HashMap<String, View>,
    pub queues: HashMap<String, Queue>,
    pub info_tab: bool,
    /// The Changes tab shows only uncommitted files.
    pub only_uncommitted: bool,
    pub search: String,
    pub prompt_height: Option<f32>,
    pub prompt_collapsed: bool,
    /// Session id → its newest change when it was last open (Unix seconds), and since when
    /// this is kept: what the overview marks as finished and not yet looked at.
    pub seen: HashMap<String, u64>,
    pub seen_from: u64,
    /// Session id → the model and effort picked for it when it was made.
    pub choices: HashMap<String, crate::runner::Choice>,
    /// Models learned since scope was built (from replies or "Look for model updates"), by
    /// full name, so the dropdowns keep offering them after a restart.
    pub model_ids: Vec<String>,
}

pub fn path() -> PathBuf {
    crate::settings::config_dir().join("state.json")
}

impl State {
    /// Missing file: a fresh state. A broken one is moved aside (`state.json.bad`) so the app
    /// still starts. Restored queues come back paused, so nothing is sent unasked.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else { return Self::default() };
        match serde_json::from_str::<State>(&text) {
            Ok(mut s) => {
                s.queues.retain(|_, q| !q.is_empty());
                for q in s.queues.values_mut() {
                    q.pause("restored after a restart");
                }
                s
            }
            Err(_) => {
                let _ = std::fs::rename(path, path.with_extension("json.bad"));
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write then rename, so a crash mid-write never leaves a half file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    /// Drops what refers to sessions or files that no longer exist.
    pub fn prune(&mut self, exists: impl Fn(&str) -> bool) {
        self.views.retain(|k, _| k.starts_with("new:") || exists(k));
        self.queues.retain(|k, _| exists(k));
        self.seen.retain(|k, _| exists(k));
        if self.open.as_deref().is_some_and(|id| !exists(id)) {
            self.open = None;
        }
        for v in self.views.values_mut() {
            let before = v.files.len();
            let active = v.active.and_then(|i| v.files.get(i)).map(|f| f.path.clone());
            v.files.retain(|f| Path::new(&f.path).is_file());
            if v.files.len() != before {
                v.active = active.and_then(|p| v.files.iter().position(|f| f.path == p));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(path: &str) -> View {
        View { files: vec![OpenFile { path: path.into(), rendered: false }], active: Some(0), ..Default::default() }
    }

    #[test]
    fn each_session_keeps_its_own_tabs() {
        let mut views = Views::default();
        // A has a file open; switching to B shows nothing of A's.
        let b = views.switch(Some("A"), Some("B"), tab("/a.rs"));
        assert!(b.is_empty());
        // B opens its own file; back to A brings A's tab back as it was.
        let a = views.switch(Some("B"), Some("A"), tab("/b.rs"));
        assert_eq!(a, tab("/a.rs"));
        assert_eq!(views.0.get("B"), Some(&tab("/b.rs")));
        // Same session: nothing moves.
        assert_eq!(views.switch(Some("A"), Some("A"), tab("/x")), tab("/x"));
        // An empty view isn't stored.
        views.switch(Some("C"), None, View::default());
        assert!(!views.0.contains_key("C"));
    }

    #[test]
    fn roundtrip_pauses_queues_and_survives_a_broken_file() {
        let dir = std::env::temp_dir().join(format!("scope-state-{}", std::process::id()));
        let path = dir.join("state.json");
        let mut s = State { open: Some("s1".into()), search: "auth".into(), prompt_height: Some(120.0), ..Default::default() };
        s.views.insert("s1".into(), View { draft: "half typed".into(), ..Default::default() });
        let mut q = Queue::default();
        q.push("next".into());
        s.queues.insert("s1".into(), q);
        s.queues.insert("s2".into(), Queue::default());
        s.save(&path).unwrap();
        let back = State::load(&path);
        assert_eq!(back.open.as_deref(), Some("s1"));
        assert_eq!(back.views["s1"].draft, "half typed");
        assert_eq!(back.prompt_height, Some(120.0));
        assert!(back.queues["s1"].paused);
        assert!(!back.queues.contains_key("s2"));
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(State::load(&path), State::default());
        assert!(dir.join("state.json.bad").exists());
        assert_eq!(State::load(&dir.join("missing.json")), State::default());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prune_skips_deleted_sessions_and_missing_files() {
        let real = std::env::temp_dir().join(format!("scope-prune-{}.rs", std::process::id()));
        std::fs::write(&real, "x").unwrap();
        let mut s = State { open: Some("gone".into()), ..Default::default() };
        s.views.insert("gone".into(), View::default());
        let mut v = tab("/no/such/file.rs");
        v.files.push(OpenFile { path: real.display().to_string(), rendered: false });
        v.active = Some(1);
        s.views.insert("kept".into(), v);
        s.queues.insert("gone".into(), Queue::default());
        s.prune(|id| id == "kept");
        assert_eq!(s.open, None);
        assert!(!s.views.contains_key("gone") && s.queues.is_empty());
        assert_eq!(s.views["kept"].files.len(), 1);
        assert_eq!(s.views["kept"].active, Some(0));
        std::fs::remove_file(real).unwrap();
    }
}
