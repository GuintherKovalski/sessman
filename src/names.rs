use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A session id → text map saved as JSON: custom names, and AI-written goals.
#[derive(Default, Debug, PartialEq)]
pub struct Names(HashMap<String, String>);

impl Names {
    pub fn path() -> PathBuf {
        crate::settings::config_dir().join("names.json")
    }

    /// Same format, used for AI-written session goals (session id → goal).
    pub fn goals_path() -> PathBuf {
        crate::settings::config_dir().join("goals.json")
    }

    pub fn load(path: &Path) -> Self {
        Self(std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&self.0)?)
    }

    pub fn get(&self, id: &str) -> Option<&str> {
        self.0.get(id).map(String::as_str)
    }

    /// An empty name removes the custom name, so the default title shows again.
    pub fn set(&mut self, id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.0.remove(id);
        } else {
            self.0.insert(id.to_owned(), name.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_clear_and_roundtrip() {
        let mut n = Names::default();
        n.set("s1", "  auth refactor ");
        assert_eq!(n.get("s1"), Some("auth refactor"));
        let path = std::env::temp_dir().join(format!("scope-names-{}.json", std::process::id()));
        n.save(&path).unwrap();
        assert_eq!(Names::load(&path), n);
        std::fs::remove_file(path).unwrap();
        n.set("s1", "");
        assert_eq!(n.get("s1"), None);
    }
}
