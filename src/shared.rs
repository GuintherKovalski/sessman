//! The config files every scope on one config folder shares (several windows, or a scope
//! started while another runs): names, goals, settings, overviews, orchestrations, session
//! tags, and workspaces (`Workspaces` keeps its own copy of this). Each keeps the text it
//! last read or wrote, so it can tell when another scope changed the file: [`watch`] notices that within
//! half a second and the app takes the change, and a change made here is made on top of the
//! file's latest contents (refresh, change, save), not over them.
use eframe::egui;
use serde::{Serialize, de::DeserializeOwned};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, SystemTime};

/// A value saved as JSON in a shared file.
#[derive(Debug)]
pub struct Shared<T> {
    value: T,
    path: PathBuf,
    /// The file as last read or written here.
    disk: String,
}

impl<T: Serialize + DeserializeOwned + Default> Shared<T> {
    /// The file's value; the default if it is missing or can't be read.
    pub fn load(path: PathBuf) -> Self {
        let disk = std::fs::read_to_string(&path).unwrap_or_default();
        let value = serde_json::from_str(&disk).unwrap_or_default();
        Self { value, path, disk }
    }

    /// Takes the file's contents if another scope changed it since it was last read or
    /// written here. True if it did. A file that doesn't parse (being written by something
    /// else, or damaged) is left alone, and so is the value.
    pub fn refresh(&mut self) -> bool {
        let Ok(disk) = std::fs::read_to_string(&self.path) else { return false };
        if disk == self.disk {
            return false;
        }
        let Ok(value) = serde_json::from_str(&disk) else { return false };
        self.value = value;
        self.disk = disk;
        true
    }

    /// Writes through a temporary file, so another scope never reads half of it.
    pub fn save(&mut self) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.value)?;
        write_whole(&self.path, &json)?;
        self.disk = json;
        Ok(())
    }

    /// Takes other scopes' changes, makes this one on top of them, and saves.
    pub fn change<R>(&mut self, f: impl FnOnce(&mut T) -> R) -> std::io::Result<R> {
        self.refresh();
        let r = f(&mut self.value);
        self.save()?;
        Ok(r)
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

/// Writes `text` to `path` through a temporary file next to it.
pub fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// How often [`watch`] looks at the files.
const EVERY: Duration = Duration::from_millis(500);

/// Looks at `paths` on a thread and sends a message (and wakes the app) when any of them is
/// written: its modification time or size changed, or it appeared or went. The app's own
/// writes are reported too; refreshing finds them unchanged.
pub fn watch(ctx: egui::Context, paths: Vec<PathBuf>) -> Receiver<()> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let stamp = |p: &PathBuf| std::fs::metadata(p).ok().map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()));
        let mut last: Vec<_> = paths.iter().map(stamp).collect();
        loop {
            std::thread::sleep(EVERY);
            let now: Vec<_> = paths.iter().map(stamp).collect();
            if now != last {
                last = now;
                if tx.send(()).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    type Map = HashMap<String, String>;

    fn path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("scope-shared-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("file.json")
    }

    #[test]
    fn a_change_is_made_on_top_of_another_scopes() {
        let p = path("on-top");
        let mut mine: Shared<Map> = Shared::load(p.clone());
        let mut theirs: Shared<Map> = Shared::load(p.clone());
        theirs.change(|m| m.insert("a".into(), "theirs".into())).unwrap();
        mine.change(|m| m.insert("b".into(), "mine".into())).unwrap();
        let on_disk: Shared<Map> = Shared::load(p.clone());
        assert_eq!(on_disk.get("a").map(String::as_str), Some("theirs"));
        assert_eq!(on_disk.get("b").map(String::as_str), Some("mine"));
        assert!(theirs.refresh(), "the other scope takes the change");
        assert_eq!(theirs.get("b").map(String::as_str), Some("mine"));
        assert!(!theirs.refresh(), "and only once");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_file_that_does_not_parse_is_not_taken() {
        let p = path("bad");
        let mut s: Shared<Map> = Shared::load(p.clone());
        s.change(|m| m.insert("a".into(), "1".into())).unwrap();
        std::fs::write(&p, "{ half").unwrap();
        assert!(!s.refresh());
        assert_eq!(s.get("a").map(String::as_str), Some("1"));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_watcher_reports_a_write() {
        let p = path("watch");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let rx = watch(egui::Context::default(), vec![p.clone()]);
        std::thread::sleep(Duration::from_millis(100));
        write_whole(&p, "{}").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
}
