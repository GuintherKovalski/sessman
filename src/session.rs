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
}

pub fn projects_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".claude/projects")
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
                        first_prompt = typed_prompt(&v);
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
    for line in lines {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if v["isSidechain"].as_bool() == Some(true) {
            continue;
        }
        if s.cwd.is_none() {
            s.cwd = v["cwd"].as_str().map(PathBuf::from);
        }
        match v["type"].as_str() {
            Some("user") => {
                if let Some(p) = typed_prompt(&v) {
                    s.prompts.push(p.clone());
                    push(&mut s.messages, Role::User, p);
                }
            }
            Some("assistant") => {
                let msg = &v["message"];
                if let Some(m) = msg["model"].as_str().filter(|m| !m.starts_with('<')) {
                    s.model = Some(m.to_owned());
                }
                let u = &msg["usage"];
                let tokens: u64 = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
                    .iter()
                    .filter_map(|k| u[k].as_u64())
                    .sum();
                if tokens > 0 {
                    s.context_tokens = tokens;
                }
                let Some(blocks) = v["message"]["content"].as_array() else { continue };
                for b in blocks {
                    match b["type"].as_str() {
                        Some("text") => push(&mut s.messages, Role::Assistant, str_of(&b["text"])),
                        Some("tool_use") => {
                            let name = str_of(&b["name"]);
                            push(&mut s.messages, Role::Assistant, format!("[tool: {name}]"));
                            collect_changes(&name, &b["input"], &mut s.changes);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    s
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

/// The text a human typed, or None for tool results, meta lines and command wrappers.
fn typed_prompt(v: &Value) -> Option<String> {
    if v["type"] != "user" || v["isMeta"].as_bool() == Some(true) {
        return None;
    }
    let content = &v["message"]["content"];
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .map(|b| str_of(&b["text"]))
            .collect::<Vec<_>>()
            .join("\n"),
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

/// Consecutive messages from the same role are merged into one bubble.
fn push(messages: &mut Vec<Message>, role: Role, text: String) {
    match messages.last_mut() {
        Some(last) if last.role == role => {
            last.text.push('\n');
            last.text.push_str(&text);
        }
        _ => messages.push(Message { role, text }),
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

    #[test]
    fn multiedit_yields_one_change_per_edit() {
        let s = run(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"MultiEdit","input":{"file_path":"c.rs","edits":[{"old_string":"a","new_string":"b"},{"old_string":"c","new_string":"d"}]}}]}}"#,
        ]);
        assert_eq!(s.changes.len(), 2);
        assert!(s.changes.iter().all(|c| c.file == "c.rs"));
    }
}
