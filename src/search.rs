//! Session search: live matching (names, then goals, then content) over an in-memory index
//! kept fresh by a background thread, and the prompt/reply format for asking Haiku.
use crate::session::{self, SessionMeta};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, SystemTime};

const MAX_TEXT: usize = 200_000;
const REFRESH: Duration = Duration::from_secs(10);

/// What search knows about one session's content.
pub struct Entry {
    mtime: SystemTime,
    /// Prompts and replies without tool lines, capped at MAX_TEXT.
    text: String,
    /// `text` ASCII-lowercased, so byte offsets match `text` for snippets.
    lower: String,
    /// The first few prompts, shortened, for the AI catalog.
    pub prompts: Vec<String>,
}

impl Entry {
    pub fn new(text: &str, prompts: &[String]) -> Self {
        let mut end = text.len().min(MAX_TEXT);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let text = text[..end].to_owned();
        let prompts = prompts.iter().take(3).map(|p| p.chars().take(150).collect::<String>().replace('\n', " ")).collect();
        Self { mtime: SystemTime::UNIX_EPOCH, lower: text.to_ascii_lowercase(), text, prompts }
    }
}

pub type Index = HashMap<String, Arc<Entry>>;

/// The session list and its content index, as of one indexer pass.
pub struct Snapshot {
    pub sessions: Vec<SessionMeta>,
    pub index: Index,
}

/// Lists sessions every 10 s (or when poked), re-reading only files that changed, and sends a
/// snapshot whenever something did. This is also what keeps the session list fresh.
pub fn spawn_indexer(ctx: eframe::egui::Context) -> (Receiver<Snapshot>, Sender<()>) {
    let (tx, rx) = channel();
    let (poke, poked) = channel::<()>();
    std::thread::spawn(move || {
        let mut index = Index::new();
        let mut last: Vec<(String, SystemTime)> = Vec::new();
        loop {
            let sessions = session::list(&session::projects_dir());
            let mut changed = false;
            for s in &sessions {
                if index.get(&s.id).is_some_and(|e| e.mtime == s.modified) {
                    continue;
                }
                let Ok(loaded) = session::load(&s.path) else { continue };
                let text: String = loaded
                    .messages
                    .iter()
                    .flat_map(|m| m.text.lines().filter(|l| !l.starts_with("[tool:")))
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut e = Entry::new(&text, &loaded.prompts);
                e.mtime = s.modified;
                index.insert(s.id.clone(), Arc::new(e));
                changed = true;
            }
            index.retain(|id, _| sessions.iter().any(|s| &s.id == id));
            let now: Vec<_> = sessions.iter().map(|s| (s.id.clone(), s.modified)).collect();
            if changed || now != last {
                last = now;
                if tx.send(Snapshot { sessions, index: index.clone() }).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
            match poked.recv_timeout(REFRESH) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    (rx, poke)
}

/// How well `text` matches `query`: 100 contains it, 80 contains every word, 50 every word is
/// in it or a word of it is a close spelling. None if not a match.
pub fn score(query: &str, text: &str) -> Option<u32> {
    let q = query.trim().to_lowercase();
    let t = text.to_lowercase();
    if q.is_empty() {
        return None;
    }
    if t.contains(&q) {
        return Some(100);
    }
    let words: Vec<&str> = q.split_whitespace().collect();
    if words.iter().all(|w| t.contains(w)) {
        return Some(80);
    }
    let text_words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let close = |w: &str| {
        let allowed = match w.chars().count() {
            0..=3 => return false,
            4..=7 => 1,
            _ => 2,
        };
        text_words.iter().any(|tw| edit_distance(w, tw) <= allowed)
    };
    words.iter().all(|w| t.contains(w) || close(w)).then_some(50)
}

/// Edits (insert, delete, substitute, swap two neighbors) to turn `a` into `b`; 3+ means "far".
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > 2 {
        return 3;
    }
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    d[0] = (0..=b.len()).collect();
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j - 1] + cost).min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// A snippet around the first word if every word of `query` occurs in the content.
pub fn content_hit(query: &str, e: &Entry) -> Option<String> {
    let q = query.trim().to_ascii_lowercase();
    if q.chars().count() < 2 {
        return None;
    }
    let words: Vec<&str> = q.split_whitespace().collect();
    if !words.iter().all(|w| e.lower.contains(w)) {
        return None;
    }
    let pos = e.lower.find(words[0])?;
    let mut start = pos.saturating_sub(30);
    while !e.text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (pos + words[0].len() + 50).min(e.text.len());
    while !e.text.is_char_boundary(end) {
        end += 1;
    }
    let mut snippet = e.text[start..end].split_whitespace().collect::<Vec<_>>().join(" ");
    if start > 0 {
        snippet.insert(0, '…');
    }
    if end < e.text.len() {
        snippet.push('…');
    }
    Some(snippet)
}

#[derive(Debug, PartialEq, Clone, Copy, PartialOrd, Eq, Ord)]
pub enum Source {
    Name,
    Goal,
    Content,
}

/// One session to search: its index into the session list and what can match.
pub struct Candidate<'a> {
    pub index: usize,
    pub name: &'a str,
    pub goal: Option<&'a str>,
    pub entry: Option<&'a Entry>,
    pub modified: SystemTime,
}

#[derive(Debug, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub source: Source,
    /// Content snippet, or the AI's reason.
    pub note: Option<String>,
}

/// Name matches first, then goal, then content; within each, better then newer. A session
/// appears once, at its best source.
pub fn rank(query: &str, candidates: &[Candidate]) -> Vec<Hit> {
    let mut hits: Vec<(Source, u32, SystemTime, Hit)> = Vec::new();
    for c in candidates {
        let found = if let Some(s) = score(query, c.name) {
            Some((Source::Name, s, None))
        } else if let Some(s) = c.goal.and_then(|g| score(query, g)) {
            Some((Source::Goal, s, None))
        } else {
            c.entry.and_then(|e| content_hit(query, e)).map(|snip| (Source::Content, 0, Some(snip)))
        };
        if let Some((source, s, note)) = found {
            hits.push((source, s, c.modified, Hit { index: c.index, source, note }));
        }
    }
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    hits.into_iter().map(|h| h.3).collect()
}

/// A numbered catalog of sessions for the model: name, goal and first prompts.
pub fn ai_prompt(query: &str, catalog: &[(String, Option<String>, Vec<String>)]) -> String {
    let mut out = format!(
        "Below is a numbered list of a user's recent Claude Code sessions. Which ones match \
         what the user is looking for?\n\nLooking for: {query}\n\n\
         Reply with up to 8 lines, best match first, each exactly `N: reason` where N is the \
         session number and reason is at most 12 words. Reply NONE if nothing matches. \
         No other text.\n\nSessions:\n"
    );
    for (i, (name, goal, prompts)) in catalog.iter().enumerate() {
        out.push_str(&format!("{}. {name}", i + 1));
        if let Some(g) = goal {
            out.push_str(&format!(" | goal: {}", g.replace('\n', " ")));
        }
        if !prompts.is_empty() {
            out.push_str(&format!(" | prompts: {}", prompts.join(" / ")));
        }
        out.push('\n');
    }
    out
}

/// `N: reason` lines → (0-based catalog index, reason). Junk and out-of-range numbers are skipped.
pub fn parse_ai(reply: &str, n: usize) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for line in reply.lines() {
        let line = line.trim().trim_start_matches(['-', '*', ' ']);
        let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
        let Ok(num) = digits.parse::<usize>() else { continue };
        let rest = line[digits.len()..].trim_start();
        let Some(reason) = rest.strip_prefix([':', '.', ')', '-']) else { continue };
        if (1..=n).contains(&num) && out.iter().all(|(i, _)| *i != num - 1) {
            out.push((num - 1, reason.trim().to_owned()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_substring_words_and_typos() {
        assert_eq!(score("markdown", "Markdown rendering and theming"), Some(100));
        assert_eq!(score("theming markdown", "Markdown rendering and theming"), Some(80));
        assert_eq!(score("rendring", "Markdown rendering and theming"), Some(50));
        assert_eq!(score("thmeing markdwon", "Markdown rendering and theming"), Some(50));
        assert_eq!(score("cat", "Markdown rendering"), None);
        assert_eq!(score("pipeline", "Markdown rendering"), None);
        assert_eq!(score("  ", "anything"), None);
    }

    #[test]
    fn content_hits_need_every_word_and_give_a_snippet() {
        let text = format!("{}We fixed the Login timeout by raising the limit.\nThen tests passed.", "x ".repeat(40));
        let e = Entry::new(&text, &[]);
        let snip = content_hit("login timeout", &e).unwrap();
        assert!(snip.contains("Login timeout") && snip.starts_with('…'), "{snip}");
        assert_eq!(content_hit("login banana", &e), None);
        assert_eq!(content_hit("x", &e), None);
    }

    #[test]
    fn entry_text_is_capped_on_a_char_boundary() {
        let e = Entry::new(&"é".repeat(MAX_TEXT), &["a\nb".into(), "c".into(), "d".into(), "e".into()]);
        assert!(e.text.len() <= MAX_TEXT);
        assert_eq!(e.prompts, vec!["a b", "c", "d"]);
    }

    #[test]
    fn ranks_name_then_goal_then_content_then_recency() {
        let t = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let body = Entry::new("we talked about the parser here", &[]);
        let c = [
            Candidate { index: 0, name: "Other", goal: None, entry: Some(&body), modified: t(9) },
            Candidate { index: 1, name: "Old parser work", goal: None, entry: None, modified: t(1) },
            Candidate { index: 2, name: "Misc", goal: Some("GOAL: rewrite the parser"), entry: Some(&body), modified: t(5) },
            Candidate { index: 3, name: "New parser work", goal: None, entry: None, modified: t(3) },
            Candidate { index: 4, name: "Nothing", goal: None, entry: None, modified: t(8) },
        ];
        let hits = rank("parser", &c);
        let order: Vec<_> = hits.iter().map(|h| (h.index, h.source)).collect();
        assert_eq!(order, vec![(3, Source::Name), (1, Source::Name), (2, Source::Goal), (0, Source::Content)]);
        assert!(hits[3].note.as_deref().unwrap().contains("parser"));
    }

    #[test]
    fn parses_ai_replies() {
        let reply = "2: fixed the login timeout\n- 5. added colors\n9: out of range\nnot a line\n2: duplicate\n1) the first";
        assert_eq!(parse_ai(reply, 5), vec![(1, "fixed the login timeout".into()), (4, "added colors".into()), (0, "the first".into())]);
        assert!(parse_ai("NONE", 5).is_empty());
    }

    #[test]
    fn ai_prompt_numbers_sessions() {
        let p = ai_prompt("login", &[("A".into(), Some("GOAL: x".into()), vec!["p1".into()]), ("B".into(), None, vec![])]);
        assert!(p.contains("1. A | goal: GOAL: x | prompts: p1\n2. B\n"));
    }
}
