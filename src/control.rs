//! Scope driven through Claude: the `scope list|rename|move|new|ask` commands, for the
//! sessions scope runs (told about them in their system prompt) and anyone at a terminal.
//! A command is a JSON file dropped in the inbox folder; the running app takes it, does it,
//! and writes the reply next to it, which the command prints. The orchestrator's actions
//! (orchestration.rs) are the same commands, done by the app directly.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// What can be asked of the app. Sessions are named by id, an id prefix, or their name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "do", rename_all = "lowercase")]
pub enum Command {
    /// The workspaces and their sessions, in the bar's order.
    List {
        #[serde(default)]
        workspace: Option<String>,
    },
    Rename { session: String, name: String },
    /// To a place in its workspace's bar: "top", "bottom", "up", "down" or a position from 1.
    Move { session: String, to: String },
    /// Starts a session with this first prompt, in a workspace (default: the caller's) and a
    /// folder (default: the workspace's).
    New {
        prompt: String,
        #[serde(default)]
        workspace: Option<String>,
        #[serde(default)]
        folder: Option<PathBuf>,
        #[serde(default)]
        name: Option<String>,
    },
    /// A question to a workspace's orchestrator, which may act on it too.
    Ask {
        question: String,
        #[serde(default)]
        workspace: Option<String>,
    },
}

/// A command and who sent it: the session it came from (`SCOPE_SESSION`) and its folder.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Request {
    pub command: Command,
    #[serde(default)]
    pub caller: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

/// A request taken from the inbox, and where its reply goes.
pub struct Incoming {
    pub request: Request,
    pub reply: PathBuf,
}

pub fn inbox() -> PathBuf {
    crate::settings::config_dir().join("inbox")
}

/// The env var a session scope runs finds its own id in.
pub const SESSION_VAR: &str = "SCOPE_SESSION";

/// Watches the inbox on a thread: each request found is taken out of it and passed on.
pub fn watch(ctx: eframe::egui::Context) -> Receiver<Incoming> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let dir = inbox();
        loop {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
                paths.sort();
                for path in paths {
                    // Taken by moving it aside first: another scope on the same config folder
                    // watches this inbox too, and whichever moves it first runs it.
                    let taken = path.with_extension(format!("taken-{}", std::process::id()));
                    if std::fs::rename(&path, &taken).is_err() {
                        continue;
                    }
                    let text = std::fs::read_to_string(&taken);
                    let _ = std::fs::remove_file(&taken);
                    let reply = path.with_extension("reply");
                    match text.map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Request>(&t).map_err(|e| e.to_string())) {
                        Ok(request) => {
                            if tx.send(Incoming { request, reply }).is_err() {
                                return;
                            }
                            ctx.request_repaint();
                        }
                        Err(e) => write_reply(&reply, &Err(format!("could not read the command: {e}"))),
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    rx
}

/// Writes a reply for the command waiting on it (whole, so it never reads half of one).
pub fn write_reply(path: &Path, result: &Result<String, String>) {
    let json = serde_json::to_string(&Reply::from(result.clone())).unwrap_or_default();
    let tmp = path.with_extension("reply.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[derive(Serialize, Deserialize)]
struct Reply {
    ok: bool,
    text: String,
}

impl From<Result<String, String>> for Reply {
    fn from(r: Result<String, String>) -> Self {
        match r {
            Ok(text) => Reply { ok: true, text },
            Err(text) => Reply { ok: false, text },
        }
    }
}

pub const USAGE: &str = "usage:
  scope list [--scope S]                 scopes and their sessions, newest first as the bar lists them
  scope rename <session> <name>          an empty name gives back its own title
  scope move <session> <top|bottom|up|down|N>
                                         its card's place on its scope's orchestration page (N from 1)
  scope new [--scope S] [--folder DIR] [--name NAME] <prompt>
                                         starts a session with that first prompt
  scope ask [--scope S] <question>       asks the scope's orchestrator (it may act)
<session> is an id, an id prefix, the session's name, or . for the session running this.
The scope defaults to the calling session's (--workspace/-w still work for --scope/-s).";

/// Whether `arg` is one of the commands, run from the command line.
pub fn is_command(arg: &str) -> bool {
    matches!(arg, "list" | "rename" | "move" | "new" | "ask")
}

/// Parses command-line arguments (after the program name) into a command.
pub fn parse_args(args: &[String]) -> Result<Command, String> {
    let (verb, rest) = args.split_first().ok_or_else(|| USAGE.to_owned())?;
    let mut workspace = None;
    let mut folder = None;
    let mut name = None;
    let mut words = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| it.next().cloned().ok_or_else(|| format!("{flag} needs a value"));
        match a.as_str() {
            "--scope" | "-s" | "--workspace" | "-w" => workspace = Some(value(a)?),
            "--folder" | "-f" if verb == "new" => folder = Some(PathBuf::from(value(a)?)),
            "--name" | "-n" if verb == "new" => name = Some(value(a)?),
            "--" => words.extend(it.by_ref().cloned()),
            _ => words.push(a.clone()),
        }
    }
    let session = |words: &[String]| words.first().cloned().ok_or_else(|| format!("{verb}: which session?\n\n{USAGE}"));
    Ok(match verb.as_str() {
        "list" => Command::List { workspace },
        "rename" => Command::Rename { session: session(&words)?, name: words[1..].join(" ") },
        "move" => match &words[..] {
            [s, to] => Command::Move { session: s.clone(), to: to.clone() },
            _ => return Err(format!("move: a session and where to\n\n{USAGE}")),
        },
        "new" if !words.is_empty() => Command::New { prompt: words.join(" "), workspace, folder, name },
        "ask" if !words.is_empty() => Command::Ask { question: words.join(" "), workspace },
        "new" | "ask" => return Err(format!("{verb}: what to {}?\n\n{USAGE}", if verb == "new" { "start it with" } else { "ask" })),
        _ => return Err(USAGE.to_owned()),
    })
}

/// Runs a command from the command line: hands it to the running app and prints its reply.
/// The exit code: 0 done, 1 refused or failed, 2 bad arguments.
pub fn cli(args: &[String]) -> i32 {
    let command = match parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let caller = std::env::var(SESSION_VAR).ok().filter(|s| !s.is_empty());
    let command = match command {
        // "." is the session running this.
        Command::Rename { session, name } if session == "." => Command::Rename { session: caller.clone().unwrap_or(session), name },
        Command::Move { session, to } if session == "." => Command::Move { session: caller.clone().unwrap_or(session), to },
        c => c,
    };
    let wait = if matches!(command, Command::Ask { .. }) { Duration::from_secs(300) } else { Duration::from_secs(15) };
    let request = Request { command, caller, cwd: std::env::current_dir().ok() };
    match send(&inbox(), &request, wait) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// Drops `request` in `dir` and waits for the app's reply.
fn send(dir: &Path, request: &Request, wait: Duration) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not write to {}: {e}", dir.display()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let base = dir.join(format!("{stamp}-{}", std::process::id()));
    let (path, reply) = (base.with_extension("json"), base.with_extension("reply"));
    // Written aside, then moved in, so the app never takes half of it.
    let tmp = base.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string(request).unwrap_or_default()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(&reply) {
            let _ = std::fs::remove_file(&reply);
            let r: Reply = serde_json::from_str(&text).map_err(|e| format!("unexpected reply: {e}"))?;
            return if r.ok { Ok(r.text) } else { Err(r.text) };
        }
        if start.elapsed() > wait {
            // Not taken: no app is running to do it, so it shouldn't happen later by surprise.
            if std::fs::remove_file(&path).is_ok() {
                return Err("scope is not running: start it and try again".into());
            }
            return Err("scope took the command but did not answer in time".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// What sessions scope runs are told, so Claude can manage them when asked. `exe` is how
/// to run scope.
pub fn system_prompt(exe: &str) -> String {
    format!(
        "You are running inside scope, the developer's IDE for Claude Code sessions, grouped in scopes. \
         When the developer asks you to start, rename or reorder sessions, or to ask the scope's orchestrator, \
         use its command (run it with Bash; it prints what it did):\n\
         \"{exe}\" list [--scope S]\n\
         \"{exe}\" rename <session> <name>\n\
         \"{exe}\" move <session> <top|bottom|up|down|N>\n\
         \"{exe}\" new [--scope S] [--folder DIR] [--name NAME] <first prompt>\n\
         \"{exe}\" ask [--scope S] <question>\n\
         <session> is an id, an id prefix, a session's name, or . for this session. The scope defaults to this session's."
    )
}

/// Where `to` puts a session now at `from` among `len`: "top", "bottom", "up", "down", or a
/// position from 1 (past the end is the bottom).
pub fn place(to: &str, from: usize, len: usize) -> Result<usize, String> {
    let last = len.saturating_sub(1);
    Ok(match to.trim().to_lowercase().as_str() {
        "top" | "first" => 0,
        "bottom" | "last" => last,
        "up" => from.saturating_sub(1),
        "down" => (from + 1).min(last),
        n => match n.trim_start_matches('#').parse::<usize>() {
            Ok(n) if n >= 1 => (n - 1).min(last),
            _ => return Err(format!("move where? \"{to}\": use top, bottom, up, down or a position from 1")),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn arguments_become_commands() {
        assert_eq!(parse_args(&args(&["rename", "abc", "Auth", "fixes"])), Ok(Command::Rename { session: "abc".into(), name: "Auth fixes".into() }));
        assert_eq!(parse_args(&args(&["move", "abc", "top"])), Ok(Command::Move { session: "abc".into(), to: "top".into() }));
        assert_eq!(
            parse_args(&args(&["new", "--name", "Docs", "-w", "W", "write", "the", "docs"])),
            Ok(Command::New { prompt: "write the docs".into(), workspace: Some("W".into()), folder: None, name: Some("Docs".into()) })
        );
        assert_eq!(parse_args(&args(&["ask", "how", "is", "it?"])), Ok(Command::Ask { question: "how is it?".into(), workspace: None }));
        assert_eq!(parse_args(&args(&["list"])), Ok(Command::List { workspace: None }));
        assert!(parse_args(&args(&["move", "abc"])).is_err());
        assert!(parse_args(&args(&["new"])).is_err());
        assert!(parse_args(&args(&["rename"])).is_err());
        assert!(parse_args(&args(&["frobnicate"])).is_err());
    }

    #[test]
    fn places_in_the_bar() {
        assert_eq!(place("top", 2, 4), Ok(0));
        assert_eq!(place("bottom", 0, 4), Ok(3));
        assert_eq!(place("up", 0, 4), Ok(0));
        assert_eq!(place("down", 3, 4), Ok(3));
        assert_eq!(place("2", 0, 4), Ok(1));
        assert_eq!(place("#9", 0, 4), Ok(3));
        assert!(place("0", 0, 4).is_err() && place("sideways", 0, 4).is_err());
    }

    #[test]
    fn a_command_waits_for_its_reply_and_gives_up_when_no_app_takes_it() {
        let dir = std::env::temp_dir().join(format!("scope-inbox-{}", std::process::id()));
        let request = Request { command: Command::List { workspace: None }, caller: None, cwd: None };
        // No app: the command is taken back.
        assert!(send(&dir, &request, Duration::from_millis(300)).unwrap_err().contains("not running"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        // A stand-in app answers.
        let d = dir.clone();
        let app = std::thread::spawn(move || loop {
            let found = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "json"));
            if let Some(p) = found {
                let req: Request = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
                std::fs::remove_file(&p).unwrap();
                write_reply(&p.with_extension("reply"), &Ok(format!("{:?}", req.command)));
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        });
        assert_eq!(send(&dir, &request, Duration::from_secs(5)), Ok("List { workspace: None }".into()));
        app.join().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
