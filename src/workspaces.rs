use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Workspace {
    pub name: String,
    pub sessions: Vec<String>,
    /// RGB. Optional so files from before colors existed still load; filled in on load.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Session last open in this workspace, opened again when the workspace is selected.
    #[serde(default)]
    pub last: Option<String>,
    /// Folder new sessions in this workspace start in; picked once, for the first one.
    #[serde(default)]
    pub folder: Option<PathBuf>,
    /// Whether `sessions` is the order the bar lists them in (arranged by hand); else newest
    /// first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ordered: bool,
    /// The bar's order, arranged by dragging its rows: those placed, top first. Sessions not
    /// in it (new ones) go above them, newest first. Empty: the bar is newest first. Apart
    /// from `ordered`, which orders the orchestration page's cards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bar: Vec<String>,
}

pub const PALETTE: [[u8; 3]; 10] = [
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
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Workspaces {
    pub list: Vec<Workspace>,
    /// Session last open in "All sessions".
    #[serde(default)]
    pub last_all: Option<String>,
    /// Folder last chosen in the picker, where it opens next time.
    #[serde(default)]
    pub last_folder: Option<PathBuf>,
    /// The file as last read or written here, to notice when another scope changed it.
    #[serde(skip)]
    disk: String,
}

impl PartialEq for Workspaces {
    fn eq(&self, other: &Self) -> bool {
        (&self.list, &self.last_all, &self.last_folder) == (&other.list, &other.last_all, &other.last_folder)
    }
}

impl Workspaces {
    pub fn path() -> PathBuf {
        crate::settings::config_dir().join("workspaces.json")
    }

    pub fn load(path: &Path) -> Self {
        let disk = std::fs::read_to_string(path).unwrap_or_default();
        let mut w = Self::parse(&disk).unwrap_or_default();
        w.disk = disk;
        w
    }

    fn parse(json: &str) -> Option<Self> {
        let mut w: Self = serde_json::from_str(json).ok()?;
        for i in 0..w.list.len() {
            if w.list[i].color.is_none() {
                w.list[i].color = Some(w.free_color());
            }
        }
        Some(w)
    }

    /// Takes the file's contents if another scope changed it since it was last read or
    /// written here, so a change is made on top of theirs instead of overwriting it.
    /// True if anything was taken.
    pub fn refresh(&mut self, path: &Path) -> bool {
        let Ok(disk) = std::fs::read_to_string(path) else { return false };
        if disk == self.disk {
            return false;
        }
        let Some(mut w) = Self::parse(&disk) else { return false };
        w.disk = disk;
        let changed = w != *self;
        *self = w;
        changed
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

    /// Writes through a temporary file, so a reader never sees a half-written file.
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, path)?;
        self.disk = json;
        Ok(())
    }

    pub fn add(&mut self, name: &str) -> Option<usize> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let color = Some(self.free_color());
        self.list.push(Workspace { name: name.to_owned(), sessions: Vec::new(), color, last: None, folder: None, ordered: false, bar: Vec::new() });
        Some(self.list.len() - 1)
    }

    /// A session moved in goes first in a hand-arranged workspace, as the newest does in the
    /// others; one already there stays where it is.
    pub fn move_to(&mut self, session: &str, ws: usize) {
        if ws >= self.list.len() || self.list[ws].sessions.iter().any(|s| s == session) {
            return;
        }
        self.remove(session);
        let w = &mut self.list[ws];
        if w.ordered {
            w.sessions.insert(0, session.to_owned());
        } else {
            w.sessions.push(session.to_owned());
        }
    }

    /// Lists workspace `ws` in the bar in `order` from now on. Its sessions missing from
    /// `order` keep their place after the others; ids not in it are ignored.
    pub fn arrange(&mut self, ws: usize, order: &[String]) {
        let Some(w) = self.list.get_mut(ws) else { return };
        let mut sessions: Vec<String> = order.iter().filter(|id| w.sessions.contains(id)).cloned().collect();
        sessions.extend(w.sessions.iter().filter(|id| !order.contains(id)).cloned());
        w.sessions = sessions;
        w.ordered = true;
    }

    /// Back to listing workspace `ws` newest first.
    pub fn unarrange(&mut self, ws: usize) {
        if let Some(w) = self.list.get_mut(ws) {
            w.ordered = false;
        }
    }

    /// Lists workspace `ws`'s bar in `order` (ids not in it are ignored) from now on.
    pub fn arrange_bar(&mut self, ws: usize, order: &[String]) {
        let Some(w) = self.list.get_mut(ws) else { return };
        w.bar = order.iter().filter(|id| w.sessions.contains(id)).cloned().collect();
    }

    /// Back to listing workspace `ws`'s bar newest first.
    pub fn unarrange_bar(&mut self, ws: usize) {
        if let Some(w) = self.list.get_mut(ws) {
            w.bar.clear();
        }
    }

    /// Moves the tab of workspace `from` to place `to` (counted before it is taken out).
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.list.len() {
            return;
        }
        let w = self.list.remove(from);
        let to = if to > from { to - 1 } else { to };
        self.list.insert(to.min(self.list.len()), w);
    }

    pub fn remove(&mut self, session: &str) {
        for w in &mut self.list {
            w.sessions.retain(|s| s != session);
            w.bar.retain(|s| s != session);
        }
    }

    /// The session last open in a tab (None = All sessions).
    pub fn last(&self, tab: Option<usize>) -> Option<&str> {
        match tab {
            None => self.last_all.as_deref(),
            Some(w) => self.list.get(w).and_then(|w| w.last.as_deref()),
        }
    }

    /// Remembers the session open in a tab. True if that changed (so it needs saving).
    pub fn set_last(&mut self, tab: Option<usize>, session: &str) -> bool {
        let slot = match tab {
            None => &mut self.last_all,
            Some(w) => match self.list.get_mut(w) {
                Some(w) => &mut w.last,
                None => return false,
            },
        };
        if slot.as_deref() == Some(session) {
            return false;
        }
        *slot = Some(session.to_owned());
        true
    }

    /// Remembers a folder picked for a new session, for the picker and for the workspace
    /// (None = All sessions) it was picked in.
    pub fn chose_folder(&mut self, tab: Option<usize>, dir: &Path) {
        self.last_folder = Some(dir.to_owned());
        if let Some(w) = tab.and_then(|w| self.list.get_mut(w)) {
            w.folder = Some(dir.to_owned());
        }
    }

    /// Renames workspace `ws`. The name must be new: commands find workspaces by name.
    pub fn rename(&mut self, ws: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("a scope needs a name".into());
        }
        if self.list.iter().enumerate().any(|(i, w)| i != ws && w.name.eq_ignore_ascii_case(name)) {
            return Err(format!("there is already a scope called \"{name}\""));
        }
        let w = self.list.get_mut(ws).ok_or("no such scope")?;
        w.name = name.to_owned();
        Ok(())
    }

    pub fn set_color(&mut self, ws: usize, color: [u8; 3]) {
        if let Some(w) = self.list.get_mut(ws) {
            w.color = Some(color);
        }
    }

    pub fn delete(&mut self, ws: usize) {
        if ws < self.list.len() {
            self.list.remove(ws);
        }
    }
}

/// Which session to open when a tab is selected: the one last open there if it is still in
/// the tab, else the most recent (`ids` is newest first). None for an empty tab.
/// How a scope's name shows: upper case, whatever was typed (it is kept as typed, and found
/// by it).
pub fn label(name: &str) -> String {
    name.to_uppercase()
}

/// A scope as a tab: its name in upper case on its color, outlined in it while selected. The
/// tabs at the top left and the cards of the overview are drawn with it, so they look alike.
pub fn tab(ui: &mut egui::Ui, name: &str, color: Option<[u8; 3]>, selected: bool) -> egui::Response {
    let bg = ui.painter().add(egui::Shape::Noop);
    let tab = ui.selectable_label(selected, label(name));
    if let Some([r, g, b]) = color {
        ui.painter().set(bg, egui::Shape::rect_filled(tab.rect, 3.0, egui::Color32::from_rgba_unmultiplied(r, g, b, 90)));
        if selected {
            ui.painter().rect_stroke(tab.rect, 3.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(r, g, b)), egui::StrokeKind::Inside);
        }
    }
    tab
}

pub fn pick_session(last: Option<&str>, ids: &[&str]) -> Option<usize> {
    last.and_then(|l| ids.iter().position(|id| *id == l)).or((!ids.is_empty()).then_some(0))
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
    fn each_tab_remembers_its_last_session() {
        let mut w = Workspaces::default();
        let a = w.add("a").unwrap();
        assert!(w.set_last(Some(a), "s1"));
        assert!(!w.set_last(Some(a), "s1"), "unchanged: nothing to save");
        assert!(w.set_last(None, "s9"));
        assert!(!w.set_last(Some(7), "s1"));
        assert_eq!((w.last(Some(a)), w.last(None), w.last(Some(7))), (Some("s1"), Some("s9"), None));
        // Older files without `last` still load.
        let old: Workspaces = serde_json::from_str(r#"{"list":[{"name":"x","sessions":[]}]}"#).unwrap();
        assert_eq!(old.last(Some(0)), None);
    }

    #[test]
    fn chosen_folder_is_kept_for_the_picker_and_the_workspace() {
        let mut w = Workspaces::default();
        let a = w.add("a").unwrap();
        w.chose_folder(None, Path::new("/x"));
        assert_eq!((w.last_folder.as_deref(), w.list[a].folder.as_deref()), (Some(Path::new("/x")), None));
        w.chose_folder(Some(a), Path::new("/y"));
        assert_eq!((w.last_folder.as_deref(), w.list[a].folder.as_deref()), (Some(Path::new("/y")), Some(Path::new("/y"))));
        // Older files without folders still load.
        let old: Workspaces = serde_json::from_str(r#"{"list":[{"name":"x","sessions":[]}]}"#).unwrap();
        assert_eq!((old.last_folder, old.list[0].folder.clone()), (None, None));
    }

    #[test]
    fn picks_last_session_else_newest_else_none() {
        assert_eq!(pick_session(Some("b"), &["a", "b"]), Some(1));
        assert_eq!(pick_session(Some("gone"), &["a", "b"]), Some(0));
        assert_eq!(pick_session(None, &["a"]), Some(0));
        assert_eq!(pick_session(Some("a"), &[]), None);
    }

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
    fn arranged_order_is_kept_and_newcomers_go_first() {
        let mut w = Workspaces::default();
        let a = w.add("a").unwrap();
        for s in ["s1", "s2", "s3"] {
            w.move_to(s, a);
        }
        w.arrange(a, &["s3".into(), "gone".into(), "s1".into()]);
        assert_eq!(w.list[a].sessions, vec!["s3", "s1", "s2"]);
        assert!(w.list[a].ordered);
        w.move_to("s4", a);
        w.move_to("s2", a);
        assert_eq!(w.list[a].sessions, vec!["s4", "s3", "s1", "s2"], "already there: stays put");
        w.unarrange(a);
        assert!(!w.list[a].ordered);
        let old: Workspaces = serde_json::from_str(r#"{"list":[{"name":"x","sessions":[]}]}"#).unwrap();
        assert!(!old.list[0].ordered);
    }

    #[test]
    fn the_bar_has_its_own_order_and_tabs_move() {
        let mut w = Workspaces::default();
        let a = w.add("a").unwrap();
        let b = w.add("b").unwrap();
        w.add("c");
        for s in ["s1", "s2", "s3"] {
            w.move_to(s, a);
        }
        w.arrange_bar(a, &["s3".into(), "gone".into(), "s1".into()]);
        assert_eq!(w.list[a].bar, vec!["s3", "s1"]);
        assert_eq!(w.list[a].sessions, vec!["s1", "s2", "s3"], "the cards' order is apart");
        assert!(!w.list[a].ordered);
        w.move_to("s3", b);
        assert_eq!(w.list[a].bar, vec!["s1"], "moved out: out of the bar too");
        w.unarrange_bar(a);
        assert!(w.list[a].bar.is_empty());
        let names = |w: &Workspaces| w.list.iter().map(|w| w.name.clone()).collect::<Vec<_>>();
        w.reorder(0, 3);
        assert_eq!(names(&w), ["b", "c", "a"], "to the end");
        w.reorder(2, 0);
        assert_eq!(names(&w), ["a", "b", "c"], "to the front");
        w.reorder(0, 2);
        assert_eq!(names(&w), ["b", "a", "c"], "before the third");
        w.reorder(1, 1);
        w.reorder(9, 0);
        assert_eq!(names(&w), ["b", "a", "c"], "in place, or no such tab");
    }

    #[test]
    fn rename_keeps_names_unique_and_color_can_be_set() {
        let mut w = Workspaces::default();
        let a = w.add("a").unwrap();
        w.add("b");
        assert!(w.rename(a, " ").is_err());
        assert!(w.rename(a, "B").is_err(), "taken, whatever the case");
        assert!(w.rename(a, "A").is_ok(), "its own name, recased");
        assert!(w.rename(a, " c ").is_ok());
        assert_eq!(w.list[a].name, "c");
        assert!(w.rename(9, "d").is_err());
        w.set_color(a, [1, 2, 3]);
        assert_eq!(w.list[a].color, Some([1, 2, 3]));
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
        let path = std::env::temp_dir().join(format!("scope-old-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"list":[{"name":"a","sessions":[]},{"name":"b","sessions":[]}]}"#).unwrap();
        let w = Workspaces::load(&path);
        std::fs::remove_file(path).unwrap();
        assert!(w.list[0].color.is_some() && w.list[0].color != w.list[1].color);
    }

    #[test]
    fn a_change_from_another_instance_is_kept() {
        let path = std::env::temp_dir().join(format!("scope-two-{}.json", std::process::id()));
        let mut a = Workspaces::default();
        a.add("x");
        a.save(&path).unwrap();
        let mut b = Workspaces::load(&path);
        b.move_to("from-b", 0);
        b.save(&path).unwrap();
        assert!(!b.refresh(&path), "its own write is not news");
        // `a` still has the old list in memory; it refreshes before changing anything.
        assert!(a.refresh(&path));
        a.move_to("from-a", 0);
        a.save(&path).unwrap();
        assert_eq!(Workspaces::load(&path).list[0].sessions, vec!["from-b", "from-a"]);
        // A broken file is not taken.
        std::fs::write(&path, "{").unwrap();
        assert!(!a.refresh(&path));
        assert_eq!(a.list[0].sessions.len(), 2);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn save_and_load_roundtrip() {
        let path = std::env::temp_dir().join(format!("scope-test-{}.json", std::process::id()));
        let mut w = Workspaces::default();
        let i = w.add("x").unwrap();
        w.move_to("s", i);
        w.save(&path).unwrap();
        assert_eq!(Workspaces::load(&path), w);
        std::fs::remove_file(path).unwrap();
        assert_eq!(Workspaces::load(Path::new("/nonexistent/x.json")), Workspaces::default());
    }
}
