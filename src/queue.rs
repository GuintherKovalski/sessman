use serde::{Deserialize, Serialize};

/// Prompts typed while a session is working, sent one by one as each run ends.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Queue {
    pub items: Vec<String>,
    /// Set by Ctrl+C, a failed run, or a restart: nothing is sent until resumed.
    pub paused: bool,
    /// Why it paused, shown next to "Resume queue".
    pub reason: Option<String>,
}

impl Queue {
    pub fn push(&mut self, prompt: String) {
        if !prompt.trim().is_empty() {
            self.items.push(prompt);
        }
    }

    /// After a run ends normally: the prompt to send next, unless paused.
    pub fn next(&mut self) -> Option<String> {
        if self.paused || self.items.is_empty() {
            return None;
        }
        Some(self.items.remove(0))
    }

    pub fn pause(&mut self, reason: &str) {
        if !self.items.is_empty() {
            self.paused = true;
            self.reason = Some(reason.to_owned());
        }
    }

    /// Unpauses and returns the prompt to send now.
    pub fn resume(&mut self) -> Option<String> {
        self.paused = false;
        self.reason = None;
        self.next()
    }

    /// Up in an empty input: the newest queued prompt comes back for editing.
    pub fn pull_last(&mut self) -> Option<String> {
        let p = self.items.pop();
        if self.items.is_empty() {
            self.paused = false;
            self.reason = None;
        }
        p
    }

    pub fn take(&mut self, i: usize) -> Option<String> {
        let p = (i < self.items.len()).then(|| self.items.remove(i));
        if self.items.is_empty() {
            self.paused = false;
            self.reason = None;
        }
        p
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_in_order_when_runs_end() {
        let mut q = Queue::default();
        q.push("one".into());
        q.push("  ".into());
        q.push("two".into());
        assert_eq!(q.items.len(), 2);
        assert_eq!(q.next().as_deref(), Some("one"));
        assert_eq!(q.next().as_deref(), Some("two"));
        assert_eq!(q.next(), None);
    }

    #[test]
    fn stop_or_failure_pauses_until_resumed() {
        let mut q = Queue::default();
        q.pause("stopped");
        assert!(!q.paused, "an empty queue never pauses");
        q.push("a".into());
        q.push("b".into());
        q.pause("stopped");
        assert_eq!(q.next(), None);
        assert_eq!(q.reason.as_deref(), Some("stopped"));
        assert_eq!(q.resume().as_deref(), Some("a"));
        assert_eq!(q.next().as_deref(), Some("b"));
    }

    #[test]
    fn pull_last_and_take_edit_queued_prompts() {
        let mut q = Queue::default();
        q.push("a".into());
        q.push("b".into());
        q.push("c".into());
        assert_eq!(q.pull_last().as_deref(), Some("c"));
        assert_eq!(q.take(0).as_deref(), Some("a"));
        assert_eq!(q.take(5), None);
        q.pause("x");
        assert_eq!(q.take(0).as_deref(), Some("b"));
        assert!(q.is_empty() && !q.paused);
    }
}
