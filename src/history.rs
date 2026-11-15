/// Shell-style prompt history: Up walks back, Down walks forward and finally restores the draft.
#[derive(Default)]
pub struct History {
    entries: Vec<String>,
    pos: Option<usize>,
    draft: String,
}

impl History {
    pub fn new(entries: Vec<String>) -> Self {
        Self { entries, ..Default::default() }
    }

    /// Adds what was just sent (unless it repeats the last entry) and stops walking.
    pub fn push(&mut self, entry: &str) {
        let entry = entry.trim();
        if !entry.is_empty() && self.entries.last().map(String::as_str) != Some(entry) {
            self.entries.push(entry.to_owned());
        }
        self.pos = None;
        self.draft.clear();
    }

    pub fn up(&mut self, current: &str) -> Option<String> {
        let pos = match self.pos {
            None if self.entries.is_empty() => return None,
            None => {
                self.draft = current.to_owned();
                self.entries.len() - 1
            }
            Some(0) => return None,
            Some(i) => i - 1,
        };
        self.pos = Some(pos);
        Some(self.entries[pos].clone())
    }

    pub fn down(&mut self) -> Option<String> {
        let i = self.pos?;
        if i + 1 < self.entries.len() {
            self.pos = Some(i + 1);
            Some(self.entries[i + 1].clone())
        } else {
            self.pos = None;
            Some(std::mem::take(&mut self.draft))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_back_and_restores_draft() {
        let mut h = History::new(vec!["A".into(), "B".into(), "C".into()]);
        assert_eq!(h.down(), None);
        assert_eq!(h.up("typing").as_deref(), Some("C"));
        assert_eq!(h.up("").as_deref(), Some("B"));
        assert_eq!(h.up("").as_deref(), Some("A"));
        assert_eq!(h.up(""), None);
        assert_eq!(h.down().as_deref(), Some("B"));
        assert_eq!(h.down().as_deref(), Some("C"));
        assert_eq!(h.down().as_deref(), Some("typing"));
        assert_eq!(h.down(), None);
    }

    #[test]
    fn push_adds_and_restarts_the_walk() {
        let mut h = History::default();
        h.push("A");
        h.push("A");
        h.push(" ");
        assert_eq!(h.up("").as_deref(), Some("A"));
        h.push("B");
        assert_eq!(h.up("").as_deref(), Some("B"));
        assert_eq!(h.up("").as_deref(), Some("A"));
        assert_eq!(h.up(""), None);
    }

    #[test]
    fn empty_history_does_nothing() {
        let mut h = History::default();
        assert_eq!(h.up("x"), None);
    }
}
