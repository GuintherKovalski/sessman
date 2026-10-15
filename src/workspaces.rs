use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Workspace {
    pub name: String,
    pub sessions: Vec<String>,
    /// RGB. Optional so files from before colors existed still load; filled in on load.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

const PALETTE: [[u8; 3]; 10] = [
    [86, 156, 214],
    [214, 122, 78],
    [106, 190, 112],
    [196, 112, 196],
    [220, 190, 80],
    [80, 190, 190],
    [222, 98, 120],
    [150, 130, 230],
    [160, 190, 90],
    [200, 150, 110],
];

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
        let mut w: Self =
            std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        for i in 0..w.list.len() {
            if w.list[i].color.is_none() {
                w.list[i].color = Some(w.free_color());
            }
        }
        w
    }

    /// First palette color no workspace uses; past the palette, a golden-angle hue.
    fn free_color(&self) -> [u8; 3] {
        let used: Vec<[u8; 3]> = self.list.iter().filter_map(|w| w.color).collect();
        if let Some(c) = PALETTE.iter().find(|c| !used.contains(c)) {
            return *c;
        }
        (0..)
            .map(|n| hue_to_rgb((used.len() + n) as f32 * 137.508))
            .find(|c| !used.contains(c))
            .unwrap()
    }

    /// The color of the workspace a session belongs to, if any.
    pub fn color_of(&self, session: &str) -> Option<[u8; 3]> {
        self.list.iter().find(|w| w.sessions.iter().any(|s| s == session)).and_then(|w| w.color)
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
        let color = Some(self.free_color());
        self.list.push(Workspace { name: name.to_owned(), sessions: Vec::new(), color });
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

/// Saturated, medium-bright color for a hue in degrees.
fn hue_to_rgb(hue: f32) -> [u8; 3] {
    let h = (hue % 360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let ch = |v: f32| (70.0 + v * 160.0) as u8;
    [ch(r), ch(g), ch(b)]
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
    fn colors_are_unique_and_reused_after_delete() {
        let mut w = Workspaces::default();
        for i in 0..25 {
            w.add(&format!("w{i}"));
        }
        let colors: Vec<_> = w.list.iter().map(|x| x.color.unwrap()).collect();
        for (i, c) in colors.iter().enumerate() {
            assert!(!colors[..i].contains(c), "duplicate color at {i}");
        }
        let freed = w.list[1].color;
        w.delete(1);
        w.add("new");
        assert_eq!(w.list.last().unwrap().color, freed);
        w.move_to("s", 0);
        assert_eq!(w.color_of("s"), w.list[0].color);
        assert_eq!(w.color_of("other"), None);
    }

    #[test]
    fn old_files_without_colors_get_distinct_ones() {
        let path = std::env::temp_dir().join(format!("sessman-old-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"list":[{"name":"a","sessions":[]},{"name":"b","sessions":[]}]}"#).unwrap();
        let w = Workspaces::load(&path);
        std::fs::remove_file(path).unwrap();
        assert!(w.list[0].color.is_some() && w.list[0].color != w.list[1].color);
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
