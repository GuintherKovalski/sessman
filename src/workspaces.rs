use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Workspace {
    pub name: String,
    pub sessions: Vec<String>,
}

/// User-defined groups of sessions. A session lives in at most one workspace.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Workspaces {
    pub list: Vec<Workspace>,
}

impl Workspaces {
    pub fn path() -> PathBuf {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/sessman/workspaces.json")
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }

    pub fn add(&mut self, name: &str) -> Option<usize> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        self.list.push(Workspace { name: name.to_owned(), sessions: Vec::new() });
        Some(self.list.len() - 1)
    }

    pub fn move_to(&mut self, session: &str, ws: usize) {
        if ws >= self.list.len() {
            return;
        }
        self.remove(session);
        self.list[ws].sessions.push(session.to_owned());
    }

    pub fn remove(&mut self, session: &str) {
        for w in &mut self.list {
            w.sessions.retain(|s| s != session);
        }
    }

    pub fn delete(&mut self, ws: usize) {
        if ws < self.list.len() {
            self.list.remove(ws);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_keeps_session_in_one_workspace() {
        let mut w = Workspaces::default();
        assert_eq!(w.add("  "), None);
        let a = w.add("a").unwrap();
        let b = w.add("b").unwrap();
        w.move_to("s1", a);
        w.move_to("s1", b);
        w.move_to("s1", 99);
        assert!(w.list[a].sessions.is_empty());
        assert_eq!(w.list[b].sessions, vec!["s1"]);
        w.remove("s1");
        assert!(w.list[b].sessions.is_empty());
        w.delete(a);
        assert_eq!(w.list.len(), 1);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let path = std::env::temp_dir().join(format!("sessman-test-{}.json", std::process::id()));
        let mut w = Workspaces::default();
        let i = w.add("x").unwrap();
        w.move_to("s", i);
        w.save(&path).unwrap();
        assert_eq!(Workspaces::load(&path), w);
        std::fs::remove_file(path).unwrap();
        assert_eq!(Workspaces::load(Path::new("/nonexistent/x.json")), Workspaces::default());
    }
}
