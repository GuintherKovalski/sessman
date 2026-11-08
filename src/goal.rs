//! Session goal as written by the summarizer:
//!
//! ```text
//! GOAL: one or two sentences
//! - [x] a subgoal that is done
//! - [ ] a subgoal still open
//! ```
//!
//! Stored as that raw text, so ticking a box just rewrites its `[ ]` / `[x]`.

/// What a side question about a session (Info → Ask about this goal) starts with, editable in
/// Settings → Harness. The goal, its checklist and the question follow it.
pub const QUESTION_HARNESS: &str = "Side question about this session (answer briefly; do not change any files).";

/// Most words in the goal written after a new session's first answer.
pub const FIRST_GOAL_WORDS: usize = 50;

/// The helper model's reply to `runner::first_goal` as a stored goal: one paragraph, cut to
/// `max` words (models overshoot), or None when it is empty.
pub fn from_summary(reply: &str, max: usize) -> Option<String> {
    let reply = reply.trim();
    let reply = reply.strip_prefix("GOAL:").unwrap_or(reply);
    let words: Vec<&str> = reply.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    let mut text = words[..words.len().min(max)].join(" ");
    if words.len() > max {
        text = text.trim_end_matches([',', ';', ':', '-']).to_owned() + "…";
    }
    Some(format!("GOAL: {text}"))
}

pub struct Goal {
    pub text: String,
    pub items: Vec<(bool, String)>,
}

pub fn parse(raw: &str) -> Goal {
    let mut text = Vec::new();
    let mut items = Vec::new();
    for line in raw.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("- [x]").or_else(|| line.strip_prefix("- [X]")) {
            items.push((true, rest.trim().to_owned()));
        } else if let Some(rest) = line.strip_prefix("- [ ]") {
            items.push((false, rest.trim().to_owned()));
        } else if !line.is_empty() {
            text.push(line.strip_prefix("GOAL:").unwrap_or(line).trim().to_owned());
        }
    }
    Goal { text: text.join(" "), items }
}

/// The raw text with checklist item `index` ticked or unticked.
pub fn toggle(raw: &str, index: usize) -> String {
    let mut n = 0;
    raw.lines()
        .map(|line| {
            let t = line.trim_start();
            let is_item = t.starts_with("- [ ]") || t.starts_with("- [x]") || t.starts_with("- [X]");
            if !is_item {
                return line.to_owned();
            }
            n += 1;
            if n - 1 != index {
                return line.to_owned();
            }
            let mark = if t.starts_with("- [ ]") { "- [x]" } else { "- [ ]" };
            format!("{mark}{}", &t[5..])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "GOAL: Build a session manager.\n- [x] List sessions\n- [ ] Show limits";

    #[test]
    fn parses_goal_and_items() {
        let g = parse(RAW);
        assert_eq!(g.text, "Build a session manager.");
        assert_eq!(g.items, vec![(true, "List sessions".into()), (false, "Show limits".into())]);
        // Plain text from before checklists still works.
        assert_eq!(parse("Just a goal.").items.len(), 0);
    }

    #[test]
    fn a_first_goal_is_one_paragraph_of_at_most_max_words() {
        assert_eq!(from_summary("  Build a\nsession   manager. ", 50).as_deref(), Some("GOAL: Build a session manager."));
        assert_eq!(from_summary("GOAL: one two, three four", 3).as_deref(), Some("GOAL: one two, three…"));
        assert_eq!(from_summary("one two, three four", 2).as_deref(), Some("GOAL: one two…"));
        assert_eq!(from_summary(" \n", 50), None);
        assert_eq!(parse(&from_summary("Do it.", 50).unwrap()).items.len(), 0);
    }

    #[test]
    fn toggles_one_item() {
        let t = toggle(RAW, 1);
        assert_eq!(parse(&t).items, vec![(true, "List sessions".into()), (true, "Show limits".into())]);
        assert_eq!(parse(&toggle(&t, 0)).items[0].0, false);
    }
}
