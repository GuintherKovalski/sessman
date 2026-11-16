use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone)]
pub struct SessionMeta {
    pub id: String,
    pub path: PathBuf,
    pub project: String,
    pub title: String,
    pub modified: SystemTime,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Role {
    User,
    Assistant,
}

pub struct Message {
    pub role: Role,
    pub text: String,
}

pub struct Change {
    pub file: String,
    pub old: String,
    pub new: String,
}

#[derive(Default)]
pub struct Session {
    pub cwd: Option<PathBuf>,
    pub messages: Vec<Message>,
    pub prompts: Vec<String>,
    pub changes: Vec<Change>,
    /// Model of the latest assistant reply.
    pub model: Option<String>,
    /// Tokens in context at the latest assistant reply (input incl. cache, as Claude Code counts it).
    pub context_tokens: u64,
    /// Images in the prompts that aren't saved in the images folder yet: (name, base64 data),
    /// for `images::save_all` when the session is shown.
    pub images: Vec<(String, String)>,
    /// Line of the session file where each message starts (see `delete_message`).
    pub starts: Vec<usize>,
    /// The file line being parsed.
    pub line: usize,
    /// Line of the reply's first entry, which may show nothing (thinking only), once one began.
    pub reply_from: Option<usize>,
}

pub fn projects_dir() -> PathBuf {
    crate::platform::claude_dir().join("projects")
}

/// All top-level session transcripts, newest first.
pub fn list(root: &Path) -> Vec<SessionMeta> {
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(root) else { return out };
    for project in projects.flatten() {
        let Ok(files) = std::fs::read_dir(project.path()) else { continue };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let id = path.file_stem().unwrap().to_string_lossy().into_owned();
            let Some(title) = scan_title(&path) else { continue };
            out.push(SessionMeta {
                title: if title.is_empty() { id.clone() } else { title },
                id,
                project: project.file_name().to_string_lossy().into_owned(),
                modified: file.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH),
                path,
            });
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Latest ai-title, else first typed prompt. None if the file has no conversation at all.
fn scan_title(path: &Path) -> Option<String> {
    let reader = BufReader::new(File::open(path).ok()?);
    let (mut ai_title, mut first_prompt, mut has_messages) = (None, None, false);
    for line in reader.lines().map_while(Result::ok) {
        // Cheap substring checks so we only JSON-parse the few lines that matter.
        if line.contains("\"ai-title\"") {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                ai_title = v["aiTitle"].as_str().map(str::to_owned);
            }
        } else if !has_messages || first_prompt.is_none() {
            if line.contains("\"type\":\"user\"") || line.contains("\"type\":\"assistant\"") {
                has_messages = true;
                if first_prompt.is_none() {
                    if let Ok(v) = serde_json::from_str::<Value>(&line) {
                        first_prompt = typed_prompt(&v, None);
                    }
                }
            }
        }
    }
    if !has_messages {
        return None;
    }
    let title = ai_title.or(first_prompt).unwrap_or_default();
    Some(title.lines().next().unwrap_or("").chars().take(80).collect())
}

pub fn load(path: &Path) -> std::io::Result<Session> {
    let reader = BufReader::new(File::open(path)?);
    Ok(parse(reader.lines().map_while(Result::ok)))
}

pub fn parse(lines: impl Iterator<Item = String>) -> Session {
    let mut s = Session::default();
    for (i, line) in lines.enumerate() {
        s.line = i;
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            s.apply(&v);
        }
    }
    s
}

impl Session {
    /// Adds one session-file line. `claude -p --output-format stream-json` emits its
    /// `assistant` / `user` lines in the same shape, so a running reply is applied the same way.
    pub fn apply(&mut self, v: &Value) {
        if v["isSidechain"].as_bool() == Some(true) {
            return;
        }
        if self.cwd.is_none() {
            self.cwd = v["cwd"].as_str().map(PathBuf::from);
        }
        match v["type"].as_str() {
            Some("user") => {
                if let Some(p) = typed_prompt(v, Some(&mut self.images)) {
                    self.push_prompt(p);
                }
            }
            Some("assistant") => {
                if self.messages.last().is_none_or(|m| m.role == Role::User) {
                    self.reply_from.get_or_insert(self.line);
                }
                let msg = &v["message"];
                if let Some(m) = msg["model"].as_str().filter(|m| !m.starts_with('<')) {
                    self.model = Some(m.to_owned());
                }
                let u = &msg["usage"];
                let tokens: u64 = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
                    .iter()
                    .filter_map(|k| u[k].as_u64())
                    .sum();
                if tokens > 0 {
                    self.context_tokens = tokens;
                }
                let Some(blocks) = msg["content"].as_array() else { return };
                for b in blocks {
                    match b["type"].as_str() {
                        Some("text") => self.push(Role::Assistant, str_of(&b["text"])),
                        Some("tool_use") => {
                            let name = str_of(&b["name"]);
                            self.push(Role::Assistant, format!("[tool: {name}]"));
                            collect_changes(&name, &b["input"], &mut self.changes);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// A prompt the user typed: added to the conversation and the prompt history.
    pub fn push_prompt(&mut self, p: String) {
        self.prompts.push(p.clone());
        self.push(Role::User, p);
    }

    /// Consecutive messages from the same role are merged into one bubble.
    fn push(&mut self, role: Role, text: String) {
        match self.messages.last_mut() {
            Some(last) if last.role == role => {
                last.text.push('\n');
                last.text.push_str(&text);
            }
            _ => {
                self.messages.push(Message { role, text });
                let from = if role == Role::User { None } else { self.reply_from };
                self.starts.push(from.unwrap_or(self.line));
                self.reply_from = None;
            }
        }
    }
}

/// Takes message `index` (as `load` numbers them, `text` being what it says) out of the session
/// file, so the next `--resume` no longer sends it to Claude. Its span runs from its first line
/// to the next message's: the user and assistant entries there go (the prompt, or the replies
/// with their tool calls and results, which only make sense together); attachments and Claude
/// Code's own records stay. Entries that pointed at a removed one point at what it pointed at.
/// The file as it was is copied to `backup` first. Refused if the message isn't there anymore,
/// or if a tool call would lose its result or a result its call (the API refuses those).
pub fn delete_message(path: &Path, index: usize, text: &str, backup: &Path) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = content.lines().collect();
    let s = parse(lines.iter().map(|l| l.to_string()));
    if s.messages.get(index).is_none_or(|m| m.text != text) {
        return Err("the session changed since it was shown; try again".into());
    }
    let span = s.starts[index]..s.starts.get(index + 1).copied().unwrap_or(lines.len());
    // Removed entry -> the entry it pointed at.
    let mut parents = std::collections::HashMap::new();
    let mut keep = vec![true; lines.len()];
    for i in span {
        let Ok(v) = serde_json::from_str::<Value>(lines[i]) else { continue };
        if matches!(v["type"].as_str(), Some("user" | "assistant"))
            && let Some(id) = v["uuid"].as_str()
        {
            parents.insert(id.to_owned(), v["parentUuid"].clone());
            keep[i] = false;
        }
    }
    if parents.is_empty() {
        return Err("the message has no entries to remove".into());
    }
    // Tool calls and results by id: (removed, kept) for each side.
    let mut tools: std::collections::HashMap<String, [[bool; 2]; 2]> = std::collections::HashMap::new();
    for (line, kept) in lines.iter().zip(&keep) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        for b in v["message"]["content"].as_array().into_iter().flatten() {
            let (side, id) = match b["type"].as_str() {
                Some("tool_use") => (0, &b["id"]),
                Some("tool_result") => (1, &b["tool_use_id"]),
                _ => continue,
            };
            tools.entry(str_of(id)).or_default()[side][*kept as usize] = true;
        }
    }
    let split = |[call, result]: &[[bool; 2]; 2]| (call[0] && result[1]) || (result[0] && call[1]);
    if tools.values().any(split) {
        return Err("a tool call of this message has its result in another one (an interrupted run); delete them together".into());
    }
    let mut out = String::with_capacity(content.len());
    for (line, _) in lines.iter().zip(&keep).filter(|(_, k)| **k) {
        out.push_str(&relink(line, &parents).unwrap_or_else(|| line.to_string()));
        out.push('\n');
    }
    if let Some(dir) = backup.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::copy(path, backup).map_err(|e| format!("could not back up the session: {e}"))?;
    let tmp = path.with_extension("jsonl.tmp");
    let perms = std::fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    std::fs::write(&tmp, out)
        .and_then(|_| std::fs::set_permissions(&tmp, perms))
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| e.to_string())
}

/// The line with its links past removed entries, or None if it links to none.
fn relink(line: &str, parents: &std::collections::HashMap<String, Value>) -> Option<String> {
    let mut v: Value = serde_json::from_str(line).ok()?;
    let mut changed = false;
    for key in ["parentUuid", "logicalParentUuid", "leafUuid"] {
        let mut to = v[key].clone();
        while let Some(up) = to.as_str().and_then(|id| parents.get(id)) {
            to = up.clone();
        }
        if to != v[key] {
            v[key] = to;
            changed = true;
        }
    }
    changed.then(|| v.to_string())
}

/// The conversation as compact text for a summarizer: each message cut to `per_message` chars,
/// and if the whole is longer than `max`, its start and (mostly) its end are kept.
pub fn digest(messages: &[Message], per_message: usize, max: usize) -> String {
    let mut out = String::new();
    for m in messages {
        let who = if m.role == Role::User { "USER" } else { "CLAUDE" };
        let text: String = m.text.lines().filter(|l| !l.starts_with("[tool:")).collect::<Vec<_>>().join(" ");
        if text.trim().is_empty() {
            continue;
        }
        let cut: String = text.chars().take(per_message).collect();
        out.push_str(&format!("{who}: {cut}\n"));
    }
    let chars: Vec<char> = out.chars().collect();
    if chars.len() <= max {
        return out;
    }
    let head: String = chars[..max / 4].iter().collect();
    let tail: String = chars[chars.len() - max * 3 / 4..].iter().collect();
    format!("{head}\n[…]\n{tail}")
}

/// The text a human typed, or None for tool results, meta lines and command wrappers. With
/// `images`, its images are markers in the text (see `images`); those not saved yet are added.
fn typed_prompt(v: &Value, mut images: Option<&mut Vec<(String, String)>>) -> Option<String> {
    if v["type"] != "user" || v["isMeta"].as_bool() == Some(true) {
        return None;
    }
    let content = &v["message"]["content"];
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            let mut pieces = Vec::new();
            for b in blocks {
                if b["type"] == "text" {
                    pieces.push(str_of(&b["text"]));
                } else if b["type"] == "image"
                    && let Some(out) = images.as_deref_mut()
                    && let Some((name, data)) = crate::images::from_block(b)
                {
                    pieces.push(crate::images::marker(&name));
                    if let Some(data) = data.filter(|_| !out.iter().any(|(n, _)| *n == name)) {
                        out.push((name, data));
                    }
                }
            }
            crate::images::join(&pieces)
        }
        _ => return None,
    };
    let t = text.trim();
    if t.is_empty() || t.starts_with('<') {
        return None;
    }
    Some(t.to_owned())
}

fn collect_changes(tool: &str, input: &Value, out: &mut Vec<Change>) {
    let file = str_of(&input["file_path"]);
    match tool {
        "Edit" => out.push(Change { file, old: str_of(&input["old_string"]), new: str_of(&input["new_string"]) }),
        "MultiEdit" => {
            for e in input["edits"].as_array().into_iter().flatten() {
                out.push(Change { file: file.clone(), old: str_of(&e["old_string"]), new: str_of(&e["new_string"]) });
            }
        }
        "Write" => out.push(Change { file, old: String::new(), new: str_of(&input["content"]) }),
        _ => {}
    }
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lines: &[&str]) -> Session {
        parse(lines.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_prompts_messages_and_changes() {
        let s = run(&[
            r#"{"type":"user","cwd":"/tmp/x","message":{"content":"fix the bug"}}"#,
            r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#,
            r#"not json"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"},{"type":"tool_use","name":"Edit","input":{"file_path":"a.rs","old_string":"x","new_string":"y"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"done"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Write","input":{"file_path":"b.rs","content":"new"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"thanks"}]}}"#,
        ]);
        assert_eq!(s.cwd, Some(PathBuf::from("/tmp/x")));
        assert_eq!(s.prompts, vec!["fix the bug", "thanks"]);
        assert_eq!(s.messages.len(), 3);
        assert_eq!(s.messages[1].text, "ok\n[tool: Edit]\n[tool: Write]");
        assert_eq!(s.changes.len(), 2);
        assert_eq!((s.changes[0].old.as_str(), s.changes[0].new.as_str()), ("x", "y"));
        assert_eq!(s.changes[1].old, "");
    }

    #[test]
    fn streamed_lines_extend_an_open_session() {
        let mut s = run(&[r#"{"type":"user","cwd":"/tmp/x","message":{"content":"first"}}"#]);
        s.push_prompt("second".into());
        for line in [
            r#"{"type":"assistant","message":{"model":"claude-a","content":[{"type":"text","text":"working"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"a.rs","old_string":"x","new_string":"y"}}]}}"#,
        ] {
            s.apply(&serde_json::from_str(line).unwrap());
        }
        assert_eq!(s.prompts, vec!["first", "second"]);
        assert_eq!(s.messages.len(), 2);
        assert_eq!(s.messages[1].text, "working\n[tool: Edit]");
        assert_eq!(s.changes.len(), 1);
        assert_eq!(s.model.as_deref(), Some("claude-a"));
    }

    #[test]
    fn images_in_prompts_are_markers_and_their_data_is_kept_once() {
        let line = r#"{"type":"user","message":{"content":[{"type":"text","text":"what is "},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0K"}},{"type":"text","text":" here"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0K"}}]}}"#;
        let s = run(&[line]);
        let name = crate::images::from_block(&serde_json::json!({"source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0K"}})).unwrap().0;
        let marker = crate::images::marker(&name);
        assert_eq!(s.prompts, vec![format!("what is {marker} here {marker}")]);
        assert_eq!(s.images.len(), 1, "the same image is saved once");
        assert_eq!(scan_title_of(line), "what is  here", "titles leave images out");
    }

    fn scan_title_of(line: &str) -> String {
        let path = std::env::temp_dir().join(format!("scope-title-{}.jsonl", std::process::id()));
        std::fs::write(&path, line).unwrap();
        let t = scan_title(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        t
    }

    #[test]
    fn digest_skips_tool_lines_and_keeps_both_ends() {
        let msgs = vec![
            Message { role: Role::User, text: "build it".into() },
            Message { role: Role::Assistant, text: "[tool: Bash]".into() },
            Message { role: Role::Assistant, text: "done\n[tool: Edit]".into() },
        ];
        assert_eq!(digest(&msgs, 100, 1000), "USER: build it\nCLAUDE: done\n");
        let long = vec![Message { role: Role::User, text: "a".repeat(50) + &"z".repeat(50) }];
        let d = digest(&long, 100, 40);
        assert!(d.starts_with("USER: aaaa") && d.trim_end().ends_with("zzzz") && d.contains("[…]"));
    }

    #[test]
    fn tracks_latest_model_and_context() {
        let s = run(&[
            r#"{"type":"assistant","message":{"model":"claude-a","usage":{"input_tokens":10,"cache_read_input_tokens":100,"output_tokens":5},"content":[]}}"#,
            r#"{"type":"assistant","message":{"model":"<synthetic>","usage":{"input_tokens":0,"output_tokens":0},"content":[]}}"#,
            r#"{"type":"assistant","message":{"model":"claude-b","usage":{"input_tokens":1,"cache_creation_input_tokens":20,"cache_read_input_tokens":200,"output_tokens":9},"content":[]}}"#,
        ]);
        assert_eq!(s.model.as_deref(), Some("claude-b"));
        assert_eq!(s.context_tokens, 221);
    }

    /// A session file: a prompt with an attachment, a reply with a tool call in between, a
    /// record that isn't conversation, then a second prompt and reply.
    const TWO_TURNS: &[&str] = &[
        r#"{"type":"user","uuid":"u1","parentUuid":null,"message":{"content":"first"}}"#,
        r#"{"type":"attachment","uuid":"a1","parentUuid":"u1"}"#,
        r#"{"type":"assistant","uuid":"c1","parentUuid":"a1","message":{"content":[{"type":"thinking","thinking":"hm"}]}}"#,
        r#"{"type":"assistant","uuid":"c2","parentUuid":"c1","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
        r#"{"type":"user","uuid":"r1","parentUuid":"c2","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#,
        r#"{"type":"assistant","uuid":"c3","parentUuid":"r1","message":{"content":[{"type":"text","text":"one"}]}}"#,
        r#"{"type":"last-prompt","lastPrompt":"first"}"#,
        r#"{"type":"user","uuid":"u2","parentUuid":"c3","message":{"content":"second"}}"#,
        r#"{"type":"assistant","uuid":"c4","parentUuid":"u2","message":{"content":[{"type":"text","text":"two"}]}}"#,
    ];

    /// Deletes message `index` from a copy of TWO_TURNS; the file's lines after, and the backup.
    fn delete_from_two_turns(name: &str, index: usize, text: &str) -> (Result<(), String>, Vec<Value>, bool) {
        let dir = std::env::temp_dir().join(format!("scope-delete-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (path, backup) = (dir.join("s.jsonl"), dir.join("backups/s.jsonl"));
        std::fs::write(&path, TWO_TURNS.join("\n") + "\n").unwrap();
        let result = delete_message(&path, index, text, &backup);
        let lines = std::fs::read_to_string(&path).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let backed_up = std::fs::read_to_string(&backup).is_ok_and(|b| b == TWO_TURNS.join("\n") + "\n");
        let _ = std::fs::remove_dir_all(&dir);
        (result, lines, backed_up)
    }

    fn uuids(lines: &[Value]) -> Vec<String> {
        lines.iter().map(|v| format!("{}<{}", v["uuid"].as_str().unwrap_or(v["type"].as_str().unwrap()), v["parentUuid"].as_str().unwrap_or("-"))).collect()
    }

    #[test]
    fn deleting_a_reply_takes_its_tool_calls_and_relinks_the_next_prompt() {
        let s = run(TWO_TURNS);
        assert_eq!(s.messages.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["first", "[tool: Bash]\none", "second", "two"]);
        assert_eq!(s.starts, [0, 2, 7, 8]);
        let (result, lines, backed_up) = delete_from_two_turns("reply", 1, "[tool: Bash]\none");
        assert_eq!(result, Ok(()));
        assert!(backed_up);
        assert_eq!(uuids(&lines), ["u1<-", "a1<u1", "last-prompt<-", "u2<a1", "c4<u2"]);
    }

    #[test]
    fn deleting_a_prompt_keeps_its_attachments_and_the_last_reply_can_go() {
        let (result, lines, _) = delete_from_two_turns("prompt", 0, "first");
        assert_eq!(result, Ok(()));
        assert_eq!(uuids(&lines)[..2], ["a1<-", "c1<a1"], "the reply's thinking is the reply's");
        let (_, lines, _) = delete_from_two_turns("last", 3, "two");
        assert_eq!(uuids(&lines).last().unwrap(), "u2<c3");
        assert_eq!(lines.len(), TWO_TURNS.len() - 1);
    }

    #[test]
    fn deleting_refuses_a_message_that_changed() {
        let (result, lines, backed_up) = delete_from_two_turns("changed", 2, "something else");
        assert!(result.is_err());
        assert_eq!(lines.len(), TWO_TURNS.len());
        assert!(!backed_up);
    }

    #[test]
    fn deleting_refuses_to_split_a_tool_call_from_its_result() {
        // An interrupted run: the next prompt was saved between a tool call and its result.
        let lines = [
            r#"{"type":"user","uuid":"u1","parentUuid":null,"message":{"content":"first"}}"#,
            r#"{"type":"assistant","uuid":"c1","parentUuid":"u1","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
            r#"{"type":"user","uuid":"u2","parentUuid":"c1","message":{"content":"second"}}"#,
            r#"{"type":"user","uuid":"r1","parentUuid":"u2","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#,
        ];
        let dir = std::env::temp_dir().join(format!("scope-delete-split-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        std::fs::write(&path, lines.join("\n")).unwrap();
        assert!(delete_message(&path, 1, "[tool: Bash]", &dir.join("b.jsonl")).unwrap_err().contains("tool call"));
        assert!(delete_message(&path, 2, "second", &dir.join("b.jsonl")).unwrap_err().contains("tool call"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), lines.join("\n"), "untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multiedit_yields_one_change_per_edit() {
        let s = run(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"MultiEdit","input":{"file_path":"c.rs","edits":[{"old_string":"a","new_string":"b"},{"old_string":"c","new_string":"d"}]}}]}}"#,
        ]);
        assert_eq!(s.changes.len(), 2);
        assert!(s.changes.iter().all(|c| c.file == "c.rs"));
    }
}

