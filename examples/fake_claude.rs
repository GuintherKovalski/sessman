//! A stand-in for `claude`, used by the app tests (src/e2e). It answers like Claude Code
//! does, without a model:
//!
//! - A prompt run (`--output-format stream-json`, with `--session-id` for a new session or
//!   `--resume` for an existing one) appends the prompt and a reply to the session file under
//!   `$HOME/.claude/projects` and streams progress the way `claude -p` does. The reply is
//!   `Echo: <first line of the prompt>`. Words in the prompt change that (plain words, so
//!   the app's Markdown shows them as they are): `HOLD` waits (after writing "Partial reply")
//!   while `$HOME/fake/hold` exists, `TOOL` starts a Bash tool call first, `EDIT` edits
//!   `notes.txt` in the session's folder, `FAIL` ends with an error, `NOFILE` fails before
//!   writing anything (so a new session never gets a file). With `--input-format stream-json`
//!   the prompt is the user message on stdin: its text, then `(N images)` when it has images,
//!   and the session file gets its content blocks as they came.
//! - A helper question (`--no-session-persistence`) gets a fixed reply in the format its
//!   parser expects (goal, refine, search, overview, orchestration, ask, btw; an empty
//!   name), or the contents of `$HOME/fake/<kind>` when that file exists. It waits while
//!   `$HOME/fake/<kind>.hold` exists.
//!
//! Every call is logged as a JSON file in `$HOME/fake/calls/`, named in call order.
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let fake = home.join("fake");
    let _ = std::fs::create_dir_all(&fake);
    log_call(&fake, &args);

    let (prompt, content) = if args.iter().any(|a| a == "--input-format") {
        stdin_prompt()
    } else {
        let p = args.last().cloned().unwrap_or_default();
        (p.clone(), Value::String(p))
    };
    let code = if args.iter().any(|a| a == "--output-format") {
        run(&home, &fake, &args, &prompt, content)
    } else {
        helper(&fake, &args, &prompt)
    };
    std::process::exit(code);
}

/// The prompt of a `--input-format stream-json` run, and its message content.
fn stdin_prompt() -> (String, Value) {
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    let v: Value = serde_json::from_str(&line).unwrap_or_default();
    let content = v["message"]["content"].clone();
    let blocks = content.as_array().cloned().unwrap_or_default();
    let text: Vec<&str> = blocks.iter().filter_map(|b| b["text"].as_str()).map(str::trim).collect();
    let n = blocks.iter().filter(|b| b["type"] == "image" && b["source"]["data"].as_str().is_some_and(|d| !d.is_empty())).count();
    let mut prompt = text.join(" ");
    if n > 0 {
        prompt.push_str(&format!(" ({n} images)"));
    }
    (prompt, content)
}

fn log_call(fake: &Path, args: &[String]) {
    let line = json!({
        "args": args,
        "cwd": std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default(),
        "session": std::env::var("SCOPE_SESSION").ok(),
    });
    // One file per call: calls run side by side, and their lines would interleave in one file.
    let dir = fake.join("calls");
    let _ = std::fs::create_dir_all(&dir);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let _ = std::fs::write(dir.join(format!("{stamp:020}-{}.json", std::process::id())), line.to_string());
}

fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn wait_while(path: &Path) {
    let start = std::time::Instant::now();
    while path.exists() && start.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Claude Code's folder name for a project: its path with `/` and `.` as `-`.
fn project_key(dir: &Path) -> String {
    dir.display().to_string().replace(['/', '.'], "-")
}

fn emit(v: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn append(file: &Path, v: &Value) {
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(file).expect("session file");
    writeln!(f, "{v}").expect("write session line");
}

fn run(home: &Path, fake: &Path, args: &[String], prompt: &str, content: Value) -> i32 {
    let projects = home.join(".claude/projects");
    let cwd = std::env::current_dir().unwrap_or_default();
    let (id, file) = match (value_after(args, "--session-id"), value_after(args, "--resume")) {
        (Some(id), _) => {
            let dir = projects.join(project_key(&cwd));
            std::fs::create_dir_all(&dir).expect("project folder");
            (id.to_owned(), dir.join(format!("{id}.jsonl")))
        }
        (None, Some(id)) => {
            let found = std::fs::read_dir(&projects)
                .into_iter()
                .flatten()
                .flatten()
                .map(|p| p.path().join(format!("{id}.jsonl")))
                .find(|p| p.exists());
            match found {
                Some(f) => (id.to_owned(), f),
                None => {
                    eprintln!("No conversation found with session ID: {id}");
                    return 1;
                }
            }
        }
        _ => {
            eprintln!("fake claude: no session given");
            return 2;
        }
    };
    if prompt.contains("NOFILE") {
        eprintln!("fake claude: failed before starting");
        return 1;
    }
    let cwd_s = cwd.display().to_string();
    let user = json!({"type": "user", "sessionId": id, "cwd": cwd_s, "message": {"role": "user", "content": content}});
    append(&file, &user);
    emit(json!({"type": "system", "subtype": "init", "session_id": id}));

    if prompt.contains("TOOL") {
        emit(json!({"type": "stream_event", "event": {"type": "content_block_start", "content_block": {"type": "tool_use", "name": "Bash"}}}));
    }
    if prompt.contains("HOLD") {
        emit(json!({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Partial reply"}}}));
        wait_while(&fake.join("hold"));
    }
    if prompt.contains("FAIL") {
        emit(json!({"type": "result", "subtype": "error", "is_error": true, "result": "fake failure"}));
        return 1;
    }

    let first = prompt.lines().next().unwrap_or_default();
    let reply = format!("Echo: {first}");
    emit(json!({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": reply}}}));
    let mut content = vec![json!({"type": "text", "text": reply})];
    if prompt.contains("EDIT") {
        let path = cwd.join("notes.txt");
        let _ = std::fs::write(&path, "after\n");
        content.push(json!({"type": "tool_use", "name": "Edit", "input": {"file_path": path.display().to_string(), "old_string": "before", "new_string": "after"}}));
    }
    let assistant = json!({
        "type": "assistant",
        "sessionId": id,
        "cwd": cwd_s,
        "message": {"role": "assistant", "model": "claude-fake-1", "usage": {"input_tokens": 1200, "output_tokens": 30}, "content": content},
    });
    append(&file, &assistant);
    emit(assistant);
    emit(json!({"type": "result", "subtype": "success", "is_error": false, "result": reply}));
    0
}

fn helper(fake: &Path, args: &[String], prompt: &str) -> i32 {
    let kind = if args.iter().any(|a| a == "--fork-session") {
        "btw"
    } else if prompt.contains("Write the session's overall goal") {
        "goal"
    } else if prompt.contains("You refine prompts") {
        "refine"
    } else if prompt.contains("Looking for:") {
        "search"
    } else if prompt.contains("overview card of a SCOPE") {
        "overview"
    } else if prompt.contains("Answer the developer's QUESTION") {
        "ask"
    } else if prompt.contains("Name this coding session") {
        "name"
    } else if prompt.contains("\"progress\": <0 to 100") {
        "orchestration"
    } else {
        "unknown"
    };
    wait_while(&fake.join(format!("{kind}.hold")));
    if let Ok(reply) = std::fs::read_to_string(fake.join(kind)) {
        print!("{reply}");
        return 0;
    }
    let reply = match kind {
        "btw" => "Fake side answer".to_owned(),
        "goal" => "GOAL: Ship the fake feature\n- [x] Write the parser\n- [ ] Add the tests".to_owned(),
        "refine" => {
            let text = prompt.rsplit("\nPROMPT:\n").next().unwrap_or_default().trim();
            json!({"prompt": format!("Refined: {text}")}).to_string()
        }
        "search" => "1: fake reason".to_owned(),
        "overview" => json!({"scope": "Fake scope", "sessions": [{"n": 1, "doing": "Fake doing", "needs": "Fake decision"}], "tickets": []}).to_string(),
        "orchestration" => json!({
            "progress": 42,
            "summary": "Fake summary of [[1]]",
            "sessions": [{"n": 1, "state": "waiting", "now": "fake doing", "next": "fake step"}],
            "attention": ["Review [[1]]"],
            "next": ["Fake next step"],
        })
        .to_string(),
        "ask" => "Fake answer about [[1]]".to_owned(),
        // Empty, so new sessions keep their first prompt as name unless a test sets one.
        "name" => String::new(),
        _ => {
            eprintln!("fake claude: unknown helper question");
            return 3;
        }
    };
    print!("{reply}");
    0
}
