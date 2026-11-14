//! Runs the whole app headless, the way a user drives it, against a throwaway home folder.
//!
//! Each test gets its own `HOME` (so `~/.claude/projects` holds only the sessions the test
//! wrote) and `SCOPE_CONFIG` (so settings, workspaces, names and state are its own), and a
//! fake `claude` (examples/fake_claude.rs) that answers without a model. The app is the real
//! one, built by `App::new` and drawn by egui_kittest; tests click, type and press keys on
//! what it shows, send `scope` commands through its inbox, and read back what it shows and
//! the files it writes.
//!
//! Nothing here reads the app's fields, so the app can be restructured freely: as long as it
//! still behaves the same, these tests pass. What the harness does rely on:
//! - `App::new(&egui::Context)` and `eframe::App` (`logic`, `ui`, `save`);
//! - the egui ids of the text fields: "prompt-input", "search", "goal-question-input",
//!   "find-input", "rename", "new-workspace";
//! - the config files' formats (they must stay readable across versions anyway).
use crate::app::App;
use eframe::egui::{self, Id, Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, Node};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

/// The app reads `HOME` and `SCOPE_CONFIG` from the process environment, so app tests take
/// turns.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

thread_local! {
    /// Whether this test thread holds ONE_AT_A_TIME (a second app in one test would wait forever).
    static HELD_HERE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// How long a wait for the app (a fake claude run, a helper answer) may take.
const PATIENCE: Duration = Duration::from_secs(15);

/// The folders of one test.
#[derive(Clone)]
pub struct Env {
    pub root: PathBuf,
    pub home: PathBuf,
    pub config: PathBuf,
    /// A project folder, where sessions run by default.
    pub proj: PathBuf,
    /// The fake claude's control folder: `hold` files and canned replies (see fake_claude.rs).
    pub fake: PathBuf,
}

impl Env {
    fn new() -> Self {
        static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // The real path (macOS: /private/var, not /var), as a run's folder reports it. Windows
        // writes one as `\\?\C:\…`, which no program reports its folder as: dropped.
        let tmp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let tmp = match tmp.to_str().and_then(|t| t.strip_prefix(r"\\?\")) {
            Some(plain) if cfg!(windows) && plain.as_bytes().get(1) == Some(&b':') => PathBuf::from(plain),
            _ => tmp,
        };
        let root = tmp.join(format!("scope-e2e-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let env = Env {
            home: root.join("home"),
            config: root.join("config"),
            proj: root.join("proj"),
            fake: root.join("home/fake"),
            root,
        };
        for d in [&env.home, &env.config, &env.proj, &env.fake, &env.projects()] {
            std::fs::create_dir_all(d).unwrap();
        }
        env
    }

    pub fn projects(&self) -> PathBuf {
        self.home.join(".claude/projects")
    }

    /// A folder for sessions, made if missing.
    pub fn folder(&self, name: &str) -> PathBuf {
        let d = self.root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Makes the fake claude wait (in `HOLD` runs, or helper questions of `kind`) until
    /// [`Env::release`] is called.
    pub fn hold(&self, what: &str) {
        std::fs::write(self.fake.join(what), "").unwrap();
    }

    pub fn release(&self, what: &str) {
        let _ = std::fs::remove_file(self.fake.join(what));
    }

    /// Makes the fake claude reply `text` to helper questions of `kind`.
    pub fn reply(&self, kind: &str, text: &str) {
        std::fs::write(self.fake.join(kind), text).unwrap();
    }

    /// Every call made to the fake claude so far: `{args, cwd, session}`.
    pub fn calls(&self) -> Vec<Value> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(self.fake.join("calls")).into_iter().flatten().flatten().map(|e| e.path()).collect();
        files.sort();
        files.iter().filter_map(|f| serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()).collect()
    }

    /// The prompt runs (not helper questions) made so far, as (session id, prompt).
    pub fn runs(&self) -> Vec<(String, String)> {
        self.calls()
            .iter()
            .filter_map(|c| {
                let args: Vec<&str> = c["args"].as_array()?.iter().filter_map(Value::as_str).collect();
                if !args.contains(&"--output-format") {
                    return None;
                }
                let at = args.iter().position(|a| *a == "--session-id" || *a == "--resume")?;
                Some((args[at + 1].to_owned(), args.last()?.to_string()))
            })
            .collect()
    }

    /// A file in the config folder, as JSON (Null if missing).
    pub fn config_json(&self, name: &str) -> Value {
        std::fs::read_to_string(self.config.join(name)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
    }

    pub fn write_config(&self, name: &str, v: &Value) {
        std::fs::write(self.config.join(name), serde_json::to_string_pretty(v).unwrap()).unwrap();
    }

    /// Starts a prompt run on session `id` the way a scope that has since quit left it: not a
    /// child of the app (`sh` starts it and exits), so the app finds it like any other run.
    #[cfg(unix)]
    pub fn run_outside(&self, id: &str, prompt: &str) {
        let script = r#""$0" -p --resume "$1" --output-format stream-json --verbose "$2" >/dev/null 2>&1 &"#;
        let status = std::process::Command::new("sh")
            .args(["-c", script])
            .arg(fake_claude())
            .args([id, prompt])
            .current_dir(&self.proj)
            .env("HOME", &self.home)
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// The session file of `id`, wherever its project is.
    pub fn session_file(&self, id: &str) -> Option<PathBuf> {
        std::fs::read_dir(self.projects()).ok()?.flatten().map(|p| p.path().join(format!("{id}.jsonl"))).find(|p| p.exists())
    }
}

/// A Claude Code session file, as Claude Code writes it.
pub struct SessionFile {
    pub id: String,
    pub cwd: PathBuf,
    title: Option<String>,
    turns: Vec<(String, String)>,
    edits: Vec<(String, String, String)>,
    model: String,
    tokens: u64,
    age_secs: u64,
}

/// A session `id` with one turn, `prompt` → `reply`, last changed `age_secs` ago.
pub fn session(id: &str, prompt: &str, age_secs: u64) -> SessionFile {
    SessionFile {
        id: id.to_owned(),
        cwd: PathBuf::new(),
        title: None,
        turns: vec![(prompt.to_owned(), format!("Reply to {prompt}"))],
        edits: Vec::new(),
        model: "claude-test-model".into(),
        tokens: 50_000,
        age_secs,
    }
}

impl SessionFile {
    /// Claude's generated title (`ai-title`), shown instead of the first prompt.
    pub fn titled(mut self, t: &str) -> Self {
        self.title = Some(t.to_owned());
        self
    }

    pub fn turn(mut self, prompt: &str, reply: &str) -> Self {
        self.turns.push((prompt.to_owned(), reply.to_owned()));
        self
    }

    /// The first turn's reply instead of the default.
    pub fn reply(mut self, reply: &str) -> Self {
        self.turns[0].1 = reply.to_owned();
        self
    }

    /// An Edit tool call in the last reply.
    pub fn edit(mut self, file: &Path, old: &str, new: &str) -> Self {
        self.edits.push((file.display().to_string(), old.to_owned(), new.to_owned()));
        self
    }

    pub fn model(mut self, m: &str, tokens: u64) -> Self {
        self.model = m.to_owned();
        self.tokens = tokens;
        self
    }

    /// Writes the file now (for sessions that appear while the app runs).
    pub fn write_to(&self, env: &Env) -> PathBuf {
        let cwd = if self.cwd.as_os_str().is_empty() { env.proj.clone() } else { self.cwd.clone() };
        let dir = env.projects().join(crate::platform::project_key(&cwd.to_string_lossy()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut lines = Vec::new();
        let cwd_s = cwd.display().to_string();
        let last = self.turns.len() - 1;
        // Each entry points at the one before it, as Claude Code chains them.
        let mut parent = Value::Null;
        for (n, (prompt, reply)) in self.turns.iter().enumerate() {
            let (u, a) = (format!("{}-u{n}", self.id), format!("{}-a{n}", self.id));
            lines.push(json!({"type": "user", "uuid": u, "parentUuid": parent, "sessionId": self.id, "cwd": cwd_s, "message": {"role": "user", "content": prompt}}));
            parent = json!(a);
            let mut content = vec![json!({"type": "text", "text": reply})];
            if n == last {
                for (file, old, new) in &self.edits {
                    content.push(json!({"type": "tool_use", "name": "Edit", "input": {"file_path": file, "old_string": old, "new_string": new}}));
                }
            }
            lines.push(json!({
                "type": "assistant", "uuid": a, "parentUuid": u, "sessionId": self.id, "cwd": cwd_s,
                "message": {"role": "assistant", "model": self.model, "usage": {"input_tokens": self.tokens}, "content": content},
            }));
        }
        if let Some(t) = &self.title {
            lines.push(json!({"type": "ai-title", "aiTitle": t, "sessionId": self.id}));
        }
        let path = dir.join(format!("{}.jsonl", self.id));
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&path, text).unwrap();
        set_age(&path, self.age_secs);
        path
    }
}

/// Sets a file's modification time to `secs` ago (the session list is newest first).
pub fn set_age(path: &Path, secs: u64) {
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(secs)).unwrap();
}

/// What a test starts the app with.
pub struct Setup {
    env: Env,
    lock: MutexGuard<'static, ()>,
    settings: serde_json::Map<String, Value>,
}

/// A fresh home with no sessions, workspaces or state; add them before [`Setup::launch`].
pub fn setup() -> Setup {
    let lock = match ONE_AT_A_TIME.try_lock() {
        Ok(l) => l,
        Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => {
            assert!(!HELD_HERE.get(), "this test already runs an app: drop it before calling setup() again");
            ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
        }
    };
    HELD_HERE.set(true);
    let env = Env::new();
    let mut settings = serde_json::Map::new();
    settings.insert("claude_path".into(), json!(fake_claude().display().to_string()));
    settings.insert("notify_when_done".into(), json!(false));
    // Off, so the helper calls tests count stay theirs; verdicts.rs's tests turn it on.
    settings.insert("tag_finished".into(), json!(false));
    settings.insert("theme".into(), json!("dark"));
    Setup { env, lock, settings }
}

/// The fake claude binary (examples/fake_claude.rs), built once per test run (`cargo test`
/// doesn't build examples by itself).
fn fake_claude() -> PathBuf {
    static BIN: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BIN.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let out = std::process::Command::new(env!("CARGO"))
            .args(["build", "--quiet", "--example", "fake_claude", "--message-format=json", "--manifest-path"])
            .arg(manifest.join("Cargo.toml"))
            .output()
            .expect("run cargo to build the fake claude");
        assert!(out.status.success(), "building the fake claude failed:\n{}", String::from_utf8_lossy(&out.stderr));
        // Where cargo put it, as cargo reports it: the layout of target/ changes between Rust
        // versions, so it isn't derived from the test binary's own path.
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == "fake_claude")
            .find_map(|m| m["executable"].as_str().map(PathBuf::from))
            .expect("cargo reported no executable for the fake claude")
    })
    .clone()
}

impl Setup {
    pub fn env(&self) -> &Env {
        &self.env
    }

    pub fn session(self, s: SessionFile) -> Self {
        s.write_to(&self.env);
        self
    }

    /// Workspaces as `workspaces.json` holds them: (name, session ids).
    pub fn workspaces(self, list: &[(&str, &[&str])]) -> Self {
        let list: Vec<Value> = list.iter().map(|(name, ids)| json!({"name": name, "sessions": ids})).collect();
        self.env.write_config("workspaces.json", &json!({"list": list}));
        self
    }

    /// The whole `workspaces.json`.
    pub fn workspaces_json(self, v: Value) -> Self {
        self.env.write_config("workspaces.json", &v);
        self
    }

    /// One setting, as `settings.json` holds it.
    pub fn setting(mut self, key: &str, v: Value) -> Self {
        self.settings.insert(key.to_owned(), v);
        self
    }

    pub fn config(self, name: &str, v: Value) -> Self {
        self.env.write_config(name, &v);
        self
    }

    pub fn launch(self) -> Scope {
        self.env.write_config("settings.json", &Value::Object(self.settings));
        Scope::start(self.env, self.lock)
    }
}

/// The running app.
pub struct Scope {
    // Dropped first: the app (and the threads it started) before the folders go.
    h: Harness<'static, App>,
    pub env: Env,
    /// The last text the app put on the clipboard.
    pub copied: Option<String>,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        HELD_HERE.set(false);
        if std::env::var_os("SCOPE_E2E_KEEP").is_none() {
            let _ = std::fs::remove_dir_all(&self.env.root);
        }
    }
}

#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

struct NoStorage;

impl eframe::Storage for NoStorage {
    fn get_string(&self, _key: &str) -> Option<String> {
        None
    }
    fn set_string(&mut self, _key: &str, _value: String) {}
    fn remove_string(&mut self, _key: &str) {}
    fn flush(&mut self) {}
}

impl Scope {
    fn start(env: Env, lock: MutexGuard<'static, ()>) -> Self {
        // SAFETY: app tests hold ONE_AT_A_TIME, and no other test reads these variables.
        unsafe {
            std::env::set_var("HOME", &env.home);
            // Windows takes the home folder from USERPROFILE first (platform.rs), and anyone
            // may point Claude Code elsewhere: neither may reach the developer's own.
            std::env::set_var("USERPROFILE", &env.home);
            std::env::remove_var("CLAUDE_CONFIG_DIR");
            std::env::set_var("SCOPE_CONFIG", &env.config);
        }
        let h = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .with_step_dt(1.0 / 60.0)
            .with_max_steps(20)
            .build_eframe(|cc| App::new(&cc.egui_ctx));
        let mut s = Scope { h, env, copied: None, _lock: lock };
        s.frames(3);
        s
    }

    /// Quits and starts again on the same folders, like closing and reopening the app.
    pub fn restart(mut self) -> Self {
        self.save();
        self.restart_without_saving()
    }

    /// Like [`Scope::restart`], without saving the state first (as after a crash).
    pub fn restart_without_saving(self) -> Self {
        // Keep the folders: this Scope is dropped without its Drop cleaning up.
        let env = self.env.clone();
        let me = std::mem::ManuallyDrop::new(self);
        // SAFETY: each field is read out once and `me` is never used again.
        let (h, lock) = unsafe { (std::ptr::read(&me.h), std::ptr::read(&me._lock)) };
        drop(h);
        let mut s = Scope::start(env, lock);
        s.drain_old_inbox_watcher();
        s
    }

    /// The previous app's inbox thread still runs until it takes a command it can't hand on.
    /// Feeds it one, so the commands a test sends after a restart reach this app.
    fn drain_old_inbox_watcher(&mut self) {
        for _ in 0..3 {
            if self.try_command(json!({"do": "list"}), Duration::from_millis(800)).is_some() {
                return;
            }
        }
    }

    /// Runs `n` frames.
    pub fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.h.step();
            for c in &self.h.output().platform_output.commands {
                if let egui::OutputCommand::CopyText(t) = c {
                    self.copied = Some(t.clone());
                }
            }
        }
    }

    /// Runs frames until `cond` holds; panics (with what is on screen) after a while.
    #[track_caller]
    pub fn wait_until(&mut self, what: &str, mut cond: impl FnMut(&mut Self) -> bool) {
        let start = Instant::now();
        loop {
            self.h.step();
            if cond(self) {
                return;
            }
            if start.elapsed() > PATIENCE {
                panic!("timed out waiting until {what}\non screen:\n{}", self.screen());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Runs frames until what is on screen stays the same for a moment (background work such
    /// as the indexer has caught up).
    #[track_caller]
    pub fn settle(&mut self) {
        let start = Instant::now();
        let mut last = self.screen();
        let mut since = Instant::now();
        while since.elapsed() < Duration::from_millis(400) {
            assert!(start.elapsed() < PATIENCE, "the screen never settled:\n{last}");
            self.h.step();
            std::thread::sleep(Duration::from_millis(10));
            let now = self.screen();
            if now != last {
                last = now;
                since = Instant::now();
            }
        }
    }

    /// Everything on screen: each widget's label and value, one per line.
    pub fn screen(&self) -> String {
        let mut out = Vec::new();
        for n in self.h.query_all_by(|_| true) {
            let a = n.accesskit_node();
            let hint = a.placeholder().map(|p| format!("({p})"));
            let text = [a.label(), a.value(), hint].into_iter().flatten().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" = ");
            if !text.is_empty() {
                out.push(text);
            }
        }
        out.join("\n")
    }

    /// Whether every widget labelled exactly `label` (at least one) can be used.
    #[track_caller]
    pub fn all_enabled(&self, label: &str) -> bool {
        let nodes: Vec<_> = self.h.query_all_by_label(label).collect();
        assert!(!nodes.is_empty(), "nothing labelled \"{label}\" on screen:\n{}", self.screen());
        nodes.iter().all(|n| !n.accesskit_node().is_disabled())
    }

    /// The window's zoom factor.
    pub fn zoom(&self) -> f32 {
        self.h.ctx.zoom_factor()
    }

    /// The window's style as egui draws it now (colors, corners, spacing, text sizes).
    pub fn style(&self) -> std::sync::Arc<egui::Style> {
        self.h.ctx.global_style()
    }

    /// Whether `text` shows anywhere (part of a label, a field's contents or its hint).
    pub fn sees(&self, text: &str) -> bool {
        self.h.query_all_by(|_| true).any(|n| {
            let a = n.accesskit_node();
            a.label().is_some_and(|l| l.contains(text)) || a.value().is_some_and(|v| v.contains(text)) || a.placeholder().is_some_and(|p| p.contains(text))
        })
    }

    #[track_caller]
    pub fn wait_to_see(&mut self, text: &str) {
        let what = format!("\"{text}\" shows");
        self.wait_until(&what, |s| s.sees(text));
    }

    #[track_caller]
    pub fn wait_not_to_see(&mut self, text: &str) {
        let what = format!("\"{text}\" is gone");
        self.wait_until(&what, |s| !s.sees(text));
    }

    #[track_caller]
    fn node<'a>(&'a self, label: &'a str) -> Node<'a> {
        let mut all: Vec<Node> = self.h.query_all_by_label(label).collect();
        if all.is_empty() {
            all = self.h.query_all_by_label_contains(label).collect();
        }
        // A widget and its text can both carry the label: one place on screen is one widget.
        let mut seen = Vec::new();
        all.retain(|n| {
            let r = n.rect();
            let new = !seen.contains(&r);
            seen.push(r);
            new
        });
        // Text that reads the same as a button (a session's first prompt in the conversation
        // and its bubble): the button is what a click means.
        if all.len() > 1 && all.iter().any(|n| n.accesskit_node().role() != egui::accesskit::Role::Label) {
            all.retain(|n| n.accesskit_node().role() != egui::accesskit::Role::Label);
        }
        match all.len() {
            1 => all.remove(0),
            0 => panic!("nothing labelled \"{label}\" on screen:\n{}", self.screen()),
            n => panic!("{n} widgets labelled \"{label}\"; be more specific:\n{}", self.screen()),
        }
    }

    /// Clicks the widget labelled `label` (exactly, else the only one containing it).
    #[track_caller]
    pub fn click(&mut self, label: &str) {
        self.steady();
        self.node(label).click();
        self.frames(4);
    }

    /// Opens the submenu `name` of an open menu (egui adds a ⏵ to its label).
    #[track_caller]
    pub fn open_submenu(&mut self, name: &str) {
        self.click(&format!("{name} ⏵"));
    }

    /// Of several widgets labelled exactly `label`, the first: the sidebar's, before the main
    /// area's (a workspace tab and its overview card share a name).
    #[track_caller]
    pub fn click_first(&mut self, label: &str) {
        self.steady();
        let n = self.h.query_all_by_label(label).next().unwrap_or_else(|| panic!("nothing labelled \"{label}\" on screen:\n{}", self.screen()));
        n.click();
        self.frames(4);
    }

    /// Of several widgets labelled exactly `label`, the last: a menu's, over the rest.
    #[track_caller]
    pub fn click_last(&mut self, label: &str) {
        self.steady();
        let n = self.h.query_all_by_label(label).last().unwrap_or_else(|| panic!("nothing labelled \"{label}\" on screen:\n{}", self.screen()));
        n.click();
        self.frames(4);
    }

    #[track_caller]
    pub fn click_with(&mut self, label: &str, m: Modifiers) {
        self.steady();
        self.node(label).click_modifiers(m);
        self.frames(4);
    }

    /// A few frames, so widgets are where they will stay: some sizes are measured a frame late
    /// (the prompt input's height), and a click aimed before that lands next to its target.
    fn steady(&mut self) {
        self.frames(3);
    }

    /// Lets simulated time pass, as between a user's separate gestures (egui counts clicks
    /// less than 0.6 s apart as one double or triple click).
    fn pause(&mut self) {
        self.frames(45);
    }

    #[track_caller]
    pub fn double_click(&mut self, label: &str) {
        self.pause();
        let n = self.node(label);
        n.click();
        n.click();
        self.frames(4);
    }

    #[track_caller]
    pub fn double_click_first(&mut self, label: &str) {
        self.pause();
        let n = self.h.query_all_by_label(label).next().unwrap_or_else(|| panic!("nothing labelled \"{label}\" on screen:\n{}", self.screen()));
        n.click();
        n.click();
        self.frames(4);
    }

    #[track_caller]
    pub fn right_click(&mut self, label: &str) {
        self.steady();
        self.node(label).click_secondary();
        self.frames(4);
    }

    #[track_caller]
    pub fn right_click_first(&mut self, label: &str) {
        self.steady();
        let n = self.h.query_all_by_label(label).next().unwrap_or_else(|| panic!("nothing labelled \"{label}\" on screen:\n{}", self.screen()));
        n.click_secondary();
        self.frames(4);
    }

    /// Middle-clicks the last widget labelled exactly `label` (a main-area tab, after the
    /// side panels).
    #[track_caller]
    pub fn middle_click_last(&mut self, label: &str) {
        self.steady();
        let n = self.h.query_all_by_label(label).last().unwrap_or_else(|| panic!("nothing labelled \"{label}\" on screen:\n{}", self.screen()));
        n.click_button(egui::PointerButton::Middle);
        self.frames(4);
    }

    #[track_caller]
    pub fn middle_click(&mut self, label: &str) {
        self.steady();
        self.node(label).click_button(egui::PointerButton::Middle);
        self.frames(4);
    }

    /// Rests the pointer on the widget labelled `label`, long enough for its tooltip.
    #[track_caller]
    pub fn hover(&mut self, label: &str) {
        self.steady();
        let at = self.node(label).rect().center();
        self.h.hover_at(at);
        self.frames(60);
    }

    /// Drags the widget labelled `from` onto the one labelled `to`.
    #[track_caller]
    pub fn drag(&mut self, from: &str, to: &str) {
        self.steady();
        let (a, b) = (self.node(from).rect().center(), self.node(to).rect().center());
        self.drag_between(a, b);
    }

    /// Of several widgets labelled exactly `from` and `to`, drags the last of the first onto
    /// the last of the second: an orchestration card's, below the sidebar's.
    #[track_caller]
    pub fn drag_last(&mut self, from: &str, to: &str) {
        self.steady();
        let last = |l: &str| self.h.query_all_by_label(l).last().unwrap_or_else(|| panic!("nothing labelled \"{l}\" on screen:\n{}", self.screen())).rect().center();
        let (a, b) = (last(from), last(to));
        self.drag_between(a, b);
    }

    fn drag_between(&mut self, a: egui::Pos2, b: egui::Pos2) {
        self.h.hover_at(a);
        self.frames(1);
        self.h.drag_at(a);
        self.frames(2);
        for k in 1..=6 {
            self.h.event(egui::Event::PointerMoved(a + (b - a) * (k as f32 / 6.0)));
            self.frames(1);
        }
        self.h.drop_at(b);
        self.frames(4);
    }

    /// Where the top of the widget labelled `label` is, from the window's top.
    #[track_caller]
    pub fn top_of(&self, label: &str) -> f32 {
        self.node(label).rect().top()
    }

    /// Whether the widget labelled `label` shows as selected (a session's bubble, a tab).
    #[track_caller]
    pub fn is_selected(&self, label: &str) -> bool {
        // egui hands a widget's selected state to AccessKit as "toggled".
        let state = |n: &Node| n.accesskit_node().toggled();
        let first = self.h.query_all_by_label(label).find(|n| state(n).is_some());
        let n = first.unwrap_or_else(|| self.node(label));
        state(&n) == Some(egui::accesskit::Toggled::True)
    }

    /// Whether `first` shows above (or before) `second`, each the first widget containing it.
    #[track_caller]
    pub fn shows_before(&self, first: &str, second: &str) -> bool {
        let lines: Vec<String> = self.screen().lines().map(str::to_owned).collect();
        let at = |t: &str| lines.iter().position(|l| l.contains(t)).unwrap_or_else(|| panic!("\"{t}\" is not on screen:\n{}", self.screen()));
        at(first) < at(second)
    }

    fn field(&self, id: &str) -> Option<Node<'_>> {
        self.field_by_id(Id::new(id))
    }

    fn field_by_id(&self, id: Id) -> Option<Node<'_>> {
        let target = id.accesskit_id();
        self.h.query_by(move |n| n.locate().0 == target)
    }

    /// Types into the field with this egui id (for fields whose id is not a plain name).
    #[track_caller]
    pub fn type_in_id(&mut self, id: Id, text: &str) {
        let n = self.field_by_id(id).unwrap_or_else(|| panic!("no field {id:?} on screen:\n{}", self.screen()));
        if !n.is_focused() {
            n.focus();
            self.frames(2);
        }
        self.field_by_id(id).unwrap().type_text(text);
        self.frames(2);
    }

    /// Types into the field showing the hint `hint` (for fields without an id of their own).
    #[track_caller]
    pub fn type_in_hint(&mut self, hint: &str, text: &str) {
        let find = |s: &Self| s.h.query_by(|n| n.placeholder() == Some(hint)).map(|n| n.accesskit_node().locate().0.0);
        let id = find(self).unwrap_or_else(|| panic!("no field with the hint \"{hint}\":\n{}", self.screen()));
        // SAFETY: the bits are an egui Id's, read back from its AccessKit node.
        self.type_in_id(unsafe { Id::from_high_entropy_bits(id) }, text);
    }

    /// Replaces the text of field `id` with `text`.
    #[track_caller]
    pub fn replace_in(&mut self, id: &str, text: &str) {
        self.type_in(id, "");
        self.press_with(Modifiers::COMMAND, Key::A);
        self.press(Key::Backspace);
        if !text.is_empty() {
            self.type_in(id, text);
        }
    }

    /// The text in field `id` (see the module docs for the ids).
    #[track_caller]
    pub fn field_text(&self, id: &str) -> String {
        match self.field(id) {
            Some(n) => n.value().unwrap_or_default(),
            None => panic!("no field \"{id}\" on screen:\n{}", self.screen()),
        }
    }

    pub fn has_field(&self, id: &str) -> bool {
        self.field(id).is_some()
    }

    /// Clicks into field `id` and types `text` at the cursor.
    #[track_caller]
    pub fn type_in(&mut self, id: &str, text: &str) {
        let n = self.field(id).unwrap_or_else(|| panic!("no field \"{id}\" on screen:\n{}", self.screen()));
        if !n.is_focused() {
            n.focus();
            self.frames(2);
        }
        let n = self.field(id).unwrap();
        n.type_text(text);
        self.frames(2);
    }

    /// The prompt input's text.
    pub fn prompt(&self) -> String {
        self.field_text("prompt-input")
    }

    /// Types into the prompt input.
    pub fn type_prompt(&mut self, text: &str) {
        self.type_in("prompt-input", text);
    }

    /// Types `text` wherever the keyboard is, without clicking a field first.
    pub fn type_here(&mut self, text: &str) {
        self.h.event(egui::Event::Text(text.to_owned()));
        self.frames(3);
    }

    /// Drops a file on the window, as from Finder.
    pub fn drop_file(&mut self, path: &Path) {
        self.h.input_mut().dropped_files.push(std::sync::Arc::new(Dropped(path.to_path_buf())));
        self.frames(3);
    }

    /// Types `text` into the prompt and presses Enter.
    pub fn send(&mut self, text: &str) {
        self.type_prompt(text);
        self.press(Key::Enter);
    }

    pub fn press(&mut self, key: Key) {
        self.h.key_press(key);
        self.frames(3);
    }

    pub fn press_with(&mut self, m: Modifiers, key: Key) {
        self.h.key_press_modifiers(m, key);
        self.frames(3);
    }

    /// A `scope` command (`{"do": ...}` as the CLI sends it), through the app's inbox.
    #[track_caller]
    pub fn command(&mut self, command: Value) -> Result<String, String> {
        self.command_from(command, None, None)
    }

    /// A command as a session scope runs sends it: with its id and folder.
    #[track_caller]
    pub fn command_from(&mut self, command: Value, caller: Option<&str>, cwd: Option<&Path>) -> Result<String, String> {
        let request = json!({"command": command, "caller": caller, "cwd": cwd});
        match self.try_request(request, PATIENCE) {
            Some(r) => r,
            None => panic!("no reply to the command\non screen:\n{}", self.screen()),
        }
    }

    fn try_command(&mut self, command: Value, wait: Duration) -> Option<Result<String, String>> {
        self.try_request(json!({"command": command}), wait)
    }

    fn try_request(&mut self, request: Value, wait: Duration) -> Option<Result<String, String>> {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let inbox = self.env.config.join("inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let base = inbox.join(format!("{:020}-{n}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()));
        let tmp = base.with_extension("tmp");
        std::fs::write(&tmp, request.to_string()).unwrap();
        std::fs::rename(&tmp, base.with_extension("json")).unwrap();
        let reply = base.with_extension("reply");
        let start = Instant::now();
        while start.elapsed() < wait {
            self.h.step();
            if let Ok(text) = std::fs::read_to_string(&reply) {
                let v: Value = serde_json::from_str(&text).unwrap();
                let text = v["text"].as_str().unwrap_or_default().to_owned();
                self.frames(2);
                return Some(if v["ok"] == true { Ok(text) } else { Err(text) });
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// `scope list`, as text.
    #[track_caller]
    pub fn list(&mut self) -> String {
        self.command(json!({"do": "list"})).expect("list")
    }

    /// Saves what a restart restores (eframe does this every few seconds and on quit).
    pub fn save(&mut self) {
        let app: &mut dyn eframe::App = self.h.state_mut();
        app.save(&mut NoStorage);
    }

    /// The state file, after saving.
    pub fn saved_state(&mut self) -> Value {
        self.save();
        self.env.config_json("state.json")
    }

    /// Waits for a prompt run to end: the app shows no "working" line and the spinner is gone.
    #[track_caller]
    pub fn wait_for_runs(&mut self) {
        self.wait_until("no session is working", |s| !s.sees("Claude is working") && !s.sees("Writing…") && !s.sees("claude is working"));
    }
}
