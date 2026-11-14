//! Prompt templates, and the skills and commands of a scope's root folder: the right panel's
//! Templates tab lists them, the main area shows one, and **Create template** opens a
//! composer, a pseudo session where Claude writes a new template.
//!
//! scope only reads these files. A template is written by Claude, in a `claude -p` run whose
//! tools are cut down to reading anything and writing Markdown files in the templates folder
//! (and the root's `.gitignore`, to ignore that folder): see [`run_args`]. The composer's
//! sessions are real Claude Code sessions, so a template can be worked on over several
//! prompts; scope keeps their ids in `templates.json` and leaves them out of the session list.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where templates go, relative to the scope's root (Settings → Templates).
pub const DEFAULT_FOLDER: &str = ".scope/templates";

/// What a listed file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Template,
    Skill,
    Command,
    PersonalSkill,
    PersonalCommand,
}

impl Kind {
    pub fn heading(self) -> &'static str {
        match self {
            Kind::Template => "Prompt templates",
            Kind::Skill => "Skills",
            Kind::Command => "Commands",
            Kind::PersonalSkill => "Your skills (~/.claude)",
            Kind::PersonalCommand => "Your commands (~/.claude)",
        }
    }

    pub fn is_template(self) -> bool {
        self == Kind::Template
    }
}

/// One file in the Templates tab.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: Kind,
    pub path: PathBuf,
    /// The front matter's `name`, else the skill's folder or the file's name.
    pub name: String,
    /// The front matter's `description`, else the body's first line.
    pub description: String,
    /// Where it sits, as the tree groups it: the subproject it belongs to (its folder relative
    /// to the root), then the folders it is nested in. Empty for one at the top.
    pub folders: Vec<String>,
}

/// The templates folder of `root`: `folder` under it, or `folder` itself when absolute.
pub fn folder_in(root: &Path, folder: &str) -> PathBuf {
    let folder = folder.trim();
    let folder = if folder.is_empty() { DEFAULT_FOLDER } else { folder };
    root.join(folder)
}

/// Everything the Templates tab lists for `root`, in the order it shows them: its templates,
/// the skills and commands of its `.claude/` and of every subproject's under it, then the
/// user's own (`home/.claude/`). Sorted by place and name within each kind.
pub fn scan(root: &Path, folder: &str, home: &Path) -> Vec<Item> {
    let mut out = Vec::new();
    md_files(&folder_in(root, folder), Vec::new(), "", Kind::Template, &mut out, false);
    for (dir, at) in projects(root) {
        skills(&dir.join(".claude/skills"), at.clone(), Kind::Skill, &mut out, 0);
        md_files(&dir.join(".claude/commands"), at, "", Kind::Command, &mut out, true);
    }
    // A root that is the home folder has no separate personal ones.
    if home != root {
        skills(&home.join(".claude/skills"), Vec::new(), Kind::PersonalSkill, &mut out, 0);
        md_files(&home.join(".claude/commands"), Vec::new(), "", Kind::PersonalCommand, &mut out, true);
    }
    // A skill or command found in several subprojects (copies of the repo, worktrees) is
    // listed once: where it is nearest the root.
    let depth = |i: &Item| i.folders.first().map_or(0, |p| p.split('/').count());
    out.sort_by_key(|a| (a.kind, a.name.to_lowercase(), depth(a), a.folders.clone()));
    out.dedup_by(|b, a| a.kind == b.kind && a.name.eq_ignore_ascii_case(&b.name));
    out.sort_by_key(|a| (a.kind, a.folders.clone(), a.name.to_lowercase()));
    out
}

/// Folders not looked into for subprojects: builds, dependencies, and hidden ones.
const NOT_PROJECTS: &[&str] = &["node_modules", "dist", "build", "vendor", "venv", "__pycache__", "Pods", "DerivedData"];
/// How deep, and through how many folders, the search for subprojects goes.
const PROJECT_DEPTH: usize = 5;
const PROJECT_FOLDERS: usize = 3000;
/// How long a search for subprojects is reused (the tab reads its files every 2 s).
const PROJECTS_FOR: std::time::Duration = std::time::Duration::from_secs(30);

type Projects = Vec<(PathBuf, Vec<String>)>;
static PROJECTS: std::sync::Mutex<BTreeMap<PathBuf, (std::time::Instant, Projects)>> = std::sync::Mutex::new(BTreeMap::new());

/// `root`, then every folder under it with a `.claude/` of its own (a subproject), each with
/// where the tree puts it: nowhere for the root, its folder relative to the root for the rest.
fn projects(root: &Path) -> Projects {
    let mut cache = PROJECTS.lock().unwrap();
    if let Some((_, found)) = cache.get(root).filter(|(at, _)| at.elapsed() < PROJECTS_FOR) {
        return found.clone();
    }
    let mut out = vec![(root.to_owned(), Vec::new())];
    let mut queue = std::collections::VecDeque::from([(root.to_owned(), 0)]);
    let mut looked = 0;
    while let Some((dir, depth)) = queue.pop_front() {
        if depth >= PROJECT_DEPTH {
            continue;
        }
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            // Not through links: they can loop, or lead out of the root.
            if !e.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.starts_with("target") || NOT_PROJECTS.contains(&name.as_str()) {
                continue;
            }
            looked += 1;
            if looked > PROJECT_FOLDERS {
                break;
            }
            let p = e.path();
            if p.join(".claude").is_dir() {
                let rel = p.strip_prefix(root).map(|r| r.display().to_string()).unwrap_or_default();
                out.push((p.clone(), vec![rel]));
            }
            queue.push_back((p, depth + 1));
        }
    }
    out[1..].sort();
    cache.insert(root.to_owned(), (std::time::Instant::now(), out.clone()));
    out
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    v.sort();
    v
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `.md` files in `dir`; with `nested`, also in its subfolders, named `sub:name` as Claude
/// Code names nested commands.
fn md_files(dir: &Path, folders: Vec<String>, prefix: &str, kind: Kind, out: &mut Vec<Item>, nested: bool) {
    for p in sorted_entries(dir) {
        let file = file_name(&p);
        if file.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if nested {
                let mut inner = folders.clone();
                inner.push(file.clone());
                md_files(&p, inner, &format!("{prefix}{file}:"), kind, out, nested);
            }
            continue;
        }
        let Some(stem) = file.strip_suffix(".md") else { continue };
        out.push(item(kind, p.clone(), &format!("{prefix}{stem}"), folders.clone()));
    }
}

/// Skills: folders with a `SKILL.md`, at any depth (a folder without one can group others).
fn skills(dir: &Path, folders: Vec<String>, kind: Kind, out: &mut Vec<Item>, depth: usize) {
    for p in sorted_entries(dir).into_iter().filter(|p| p.is_dir()) {
        let name = file_name(&p);
        if name.starts_with('.') {
            continue;
        }
        let file = p.join("SKILL.md");
        if file.is_file() {
            out.push(item(kind, file, &name, folders.clone()));
        } else if depth < 4 {
            let mut inner = folders.clone();
            inner.push(name);
            skills(&p, inner, kind, out, depth + 1);
        }
    }
}

/// A row of the Templates tab's tree: a group that opens and closes, or an item (an index
/// into the list given to [`tree`]) with the label it shows.
#[derive(Debug, PartialEq)]
pub enum Node {
    Group { label: String, key: String, children: Vec<Node> },
    Leaf { item: usize, label: String },
}

impl Node {
    /// How many items it holds.
    pub fn count(&self) -> usize {
        match self {
            Node::Group { children, .. } => children.iter().map(Node::count).sum(),
            Node::Leaf { .. } => 1,
        }
    }
}

/// `items` as a tree: grouped by their folders (subproject, then nesting), and within one
/// place, the ones whose names share a prefix (`speckit-plan`, `speckit-tasks`) under it.
/// Groups come first, then the items, each by name.
pub fn tree(items: &[Item], which: &[usize]) -> Vec<Node> {
    level(items, which, 0, "")
}

fn level(items: &[Item], which: &[usize], depth: usize, key: &str) -> Vec<Node> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut here = Vec::new();
    for &i in which {
        match items[i].folders.get(depth) {
            Some(f) => groups.entry(f.clone()).or_default().push(i),
            None => here.push(i),
        }
    }
    let mut out: Vec<Node> = groups
        .into_iter()
        .map(|(label, inner)| {
            let key = format!("{key}/{label}");
            Node::Group { children: level(items, &inner, depth + 1, &key), label, key }
        })
        .collect();
    // A nested command's name carries its folders (`git:pr`); the tree shows them already.
    let short = |i: usize| items[i].name.rsplit(':').next().unwrap_or_default().to_owned();
    let prefix = |name: &str| name.split_once(['-', '_']).map(|(p, _)| p.to_owned()).filter(|p| !p.is_empty());
    let mut shared: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for &i in &here {
        if let Some(p) = prefix(&short(i)) {
            shared.entry(p).or_default().push(i);
        }
    }
    shared.retain(|_, v| v.len() >= 2);
    let mut rest: Vec<(String, Node)> = Vec::new();
    for (p, inner) in &shared {
        let children = inner
            .iter()
            .map(|&i| {
                let name = short(i);
                let label = name.get(p.len() + 1..).filter(|l| !l.is_empty()).unwrap_or(&name).to_owned();
                (label, i)
            })
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .map(|(label, item)| Node::Leaf { item, label })
            .collect();
        rest.push((p.to_lowercase(), Node::Group { label: p.clone(), key: format!("{key}/{p}-"), children }));
    }
    for &i in here.iter().filter(|i| !shared.values().any(|v| v.contains(i))) {
        rest.push((short(i).to_lowercase(), Node::Leaf { item: i, label: short(i) }));
    }
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    out.extend(rest.into_iter().map(|(_, n)| n));
    out
}

fn item(kind: Kind, path: PathBuf, fallback: &str, folders: Vec<String>) -> Item {
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let (meta, body) = front_matter(&text);
    let name = meta.get("name").filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| fallback.to_owned());
    let description = meta.get("description").filter(|d| !d.is_empty()).cloned().unwrap_or_else(|| first_line(body));
    Item { kind, path, name, description, folders }
}

/// The body's first line of text, without Markdown heading marks.
fn first_line(body: &str) -> String {
    let line = body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    line.trim_start_matches('#').trim().chars().take(160).collect()
}

/// The `key: value` lines of a `---` front matter block, and the text after it. Quotes around
/// a value are dropped. Text without front matter is all body.
pub fn front_matter(text: &str) -> (BTreeMap<String, String>, &str) {
    let mut meta = BTreeMap::new();
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else { return (meta, text) };
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        at += line.len();
        let l = line.trim_end();
        if l == "---" {
            return (meta, rest[at..].trim_start_matches(['\n', '\r']));
        }
        if let Some((k, v)) = l.split_once(':')
            && !k.starts_with([' ', '\t'])
        {
            let v = v.trim();
            let v = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(v);
            meta.insert(k.trim().to_owned(), v.to_owned());
        }
    }
    (BTreeMap::new(), text)
}

/// What **Use in prompt** puts in the input: a template's body, or `/<name> ` for a skill or
/// command (Claude Code runs it by that name).
pub fn use_text(item: &Item, text: &str) -> String {
    match item.kind {
        Kind::Template => front_matter(text).1.trim().to_owned(),
        _ => format!("/{} ", item.name),
    }
}

/// The composer's sessions (ids), kept out of the session list: the root each works in and
/// the template it wrote. Shared by every scope on the config folder, like names.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Store {
    pub sessions: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Entry {
    pub root: PathBuf,
    pub template: Option<PathBuf>,
}

impl Store {
    pub fn path() -> PathBuf {
        crate::settings::config_dir().join("templates.json")
    }

    /// The composer session that wrote `template`, if scope knows it.
    pub fn session_of(&self, template: &Path) -> Option<&str> {
        self.sessions.iter().find(|(_, e)| e.template.as_deref() == Some(template)).map(|(id, _)| id.as_str())
    }
}

/// Opens and closes what scope adds to a prompt about the sessions it refers to; the
/// composer shows prompts without it.
const REFS_OPEN: &str = "\n\n<referenced-sessions>";
const REFS_CLOSE: &str = "</referenced-sessions>";

/// A session a composer prompt refers to, as `@[its name]`.
pub struct Reference {
    pub name: String,
    pub transcript: PathBuf,
    pub folder: Option<PathBuf>,
    pub digest: String,
}

/// The names in a prompt's `@[name]` references, in order, each once.
pub fn references(prompt: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = prompt;
    while let Some(at) = rest.find("@[") {
        rest = &rest[at + 2..];
        let Some(end) = rest.find(']') else { break };
        let name = rest[..end].trim().to_owned();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        rest = &rest[end + 1..];
    }
    out
}

/// A composer prompt as the conversation shows it (Markdown): each `@[name]` as code, so it
/// reads as typed.
pub fn display(prompt: &str) -> String {
    let mut out = prompt.to_owned();
    for name in references(prompt) {
        out = out.replace(&format!("@[{name}]"), &format!("`@[{name}]`"));
    }
    out
}

/// The prompt Claude gets: the one typed, then what scope knows of each session it refers to
/// (where its transcript is, and a digest of its conversation). Err names a reference that
/// isn't a session.
pub fn expand(prompt: &str, mut find: impl FnMut(&str) -> Result<Reference, String>) -> Result<String, String> {
    let refs = references(prompt);
    if refs.is_empty() {
        return Ok(prompt.to_owned());
    }
    let mut out = format!("{prompt}{REFS_OPEN}\nThe sessions referred to above as @[name]. Each transcript is Claude Code's session file (JSON lines): read or grep it for the exact steps, tool calls and their results.\n");
    for name in refs {
        let r = find(&name).map_err(|e| format!("@[{name}]: {e}"))?;
        out.push_str(&format!("\n## @[{}]\nTranscript: {}\n", r.name, r.transcript.display()));
        if let Some(f) = &r.folder {
            out.push_str(&format!("Worked in: {}\n", f.display()));
        }
        out.push_str(&format!("Digest of the conversation:\n{}\n", r.digest.trim_end()));
    }
    out.push_str(REFS_CLOSE);
    Ok(out)
}

/// A prompt as typed: without what [`expand`] added.
pub fn typed(prompt: &str) -> &str {
    match prompt.find(REFS_OPEN) {
        Some(i) if prompt.trim_end().ends_with(REFS_CLOSE) => &prompt[..i],
        _ => prompt,
    }
}

/// What the composer's Claude is told, on top of Claude Code's own system prompt. `editing`
/// is the template it works on, once there is one.
pub fn system_prompt(root: &Path, dir: &Path, gitignore: bool, editing: Option<&Path>) -> String {
    let ignore = if gitignore {
        let rel = dir.strip_prefix(root).map(|r| r.display().to_string()).unwrap_or_else(|_| dir.display().to_string());
        format!(
            "\n- If `{}/.gitignore` doesn't ignore the templates folder yet, add the line `/{}/` to it \
             (create the file if there is none). That is the only other file you may touch.",
            root.display(),
            rel.trim_end_matches('/')
        )
    } else {
        String::new()
    };
    let target = match editing {
        Some(p) => format!("You are working on the template {}: change that file, don't start another.", p.display()),
        None => "Write one new template file, named after what it does in kebab case (e.g. `fix-flaky-test.md`).".to_owned(),
    };
    format!(
        "You are writing a PROMPT TEMPLATE for the developer, in scope (the IDE you run in). This \
         is not a coding session: don't change the project.\n\
         Templates folder: {dir}\n\
         Project root: {root}\n\n\
         Rules:\n\
         - The only files you may create or change are Markdown files in the templates folder. \
         Never edit, create or delete any other file, and don't run commands: you can read \
         anything (Read, Grep, Glob).{ignore}\n\
         - {target}\n\
         - A template is a prompt for a fresh Claude Code session, written to Claude, ready to \
         send. Use placeholders like {{{{target_file}}}} for what changes from one use to the next.\n\
         - The file starts with front matter, then the prompt:\n\
         ---\n\
         name: <a short name, a few words>\n\
         description: <one line: what it does and when to use it>\n\
         ---\n\
         <the prompt>\n\
         - When the developer refers to a session (@[its name]), its transcript and a digest come \
         with the message. To reproduce what it did, read the transcript and write a prompt that \
         would get a fresh session to the same result: the goal, the context it needs, the steps \
         that worked (skip the dead ends), and how to check it is done. Turn the specifics that \
         would differ next time into placeholders.\n\
         - When done, reply in two or three lines: the file, and what the template does. If the \
         request is unclear, ask instead of guessing.",
        dir = dir.display(),
        root = root.display(),
    )
}

/// The arguments that set up a composer run, before the prompt run's own (`runner::send_args`):
/// its instructions, only the file tools, reading allowed everywhere, writing only in the
/// templates folder (and the root's `.gitignore` with `gitignore`), anything else refused
/// without asking (`dontAsk`, after Settings' extra arguments so it wins over theirs).
pub fn run_args(root: &Path, dir: &Path, gitignore: bool, editing: Option<&Path>) -> Vec<String> {
    // A folder outside the root (an absolute one in Settings) isn't the root's to ignore.
    let gitignore = gitignore && dir.starts_with(root);
    // `//` starts an absolute path in a permission rule; an Edit rule covers every tool that
    // writes files (Write ones aren't checked against paths).
    let mut allowed = vec!["Read".to_owned(), "Grep".to_owned(), "Glob".to_owned(), format!("Edit(/{}/**)", crate::platform::rule_path(dir))];
    if gitignore {
        allowed.push(format!("Edit(/{})", crate::platform::rule_path(&root.join(".gitignore"))));
    }
    vec![
        "--append-system-prompt".into(),
        system_prompt(root, dir, gitignore, editing),
        "--tools".into(),
        "Read,Grep,Glob,Write,Edit".into(),
        "--allowedTools".into(),
        allowed.join(","),
        "--permission-mode".into(),
        "dontAsk".into(),
    ]
}

/// The template a run wrote: the file its Write / Edit calls name in `dir`, from a stream
/// line (in the session file's shape).
pub fn written(v: &serde_json::Value, dir: &Path) -> Option<PathBuf> {
    if v["type"] != "assistant" {
        return None;
    }
    v["message"]["content"].as_array()?.iter().rev().find_map(|b| {
        let tool = b["name"].as_str()?;
        if b["type"] != "tool_use" || !matches!(tool, "Write" | "Edit" | "MultiEdit") {
            return None;
        }
        let p = PathBuf::from(b["input"]["file_path"].as_str()?);
        (p.starts_with(dir) && p.extension().is_some_and(|e| e == "md")).then_some(p)
    })
}

/// The newest `.md` file in `dir` changed at or after `since`: the template a run wrote, when
/// its tool calls didn't say (a run followed from its file).
pub fn changed_since(dir: &Path, since: std::time::SystemTime) -> Option<PathBuf> {
    sorted_entries(dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .filter(|(t, _)| *t >= since)
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// The session file of `id`, wherever Claude Code put it under `projects`.
pub fn session_file(projects: &Path, id: &str) -> Option<PathBuf> {
    sorted_entries(projects).into_iter().map(|d| d.join(format!("{id}.jsonl"))).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn front_matter_gives_its_keys_and_the_body() {
        let (m, body) = front_matter("---\nname: Fix it\ndescription: \"Fixes: things\"\n---\n\nDo {{x}}.\n");
        assert_eq!((m["name"].as_str(), m["description"].as_str(), body), ("Fix it", "Fixes: things", "Do {{x}}.\n"));
        let (m, body) = front_matter("# Title\ntext");
        assert!(m.is_empty());
        assert_eq!(body, "# Title\ntext");
        // An unclosed block isn't front matter.
        assert_eq!(front_matter("---\nname: x\n").1, "---\nname: x\n");
    }

    #[test]
    fn scan_lists_templates_then_skills_and_commands_then_the_users() {
        let tmp = std::env::temp_dir().join(format!("scope-templates-{}", std::process::id()));
        let (root, home) = (tmp.join("proj"), tmp.join("home"));
        write(&root.join(".scope/templates/b.md"), "---\nname: Beta\ndescription: second\n---\nbody");
        write(&root.join(".scope/templates/a.md"), "# Alpha heading\nbody");
        write(&root.join(".scope/templates/notes.txt"), "not a template");
        write(&root.join(".claude/skills/deploy/SKILL.md"), "---\nname: deploy\ndescription: Ships it\n---\n");
        write(&root.join(".claude/skills/empty/README.md"), "no SKILL.md here");
        write(&root.join(".claude/skills/team/lint/SKILL.md"), "Lints");
        write(&root.join(".claude/commands/review.md"), "Review the diff");
        write(&root.join(".claude/commands/git/pr.md"), "Open a PR");
        // Subprojects with their own .claude, at any depth; not in builds or hidden folders.
        write(&root.join("apps/api/.claude/skills/migrate/SKILL.md"), "Migrates");
        // Copies of a skill further down are left out for the one nearest the root.
        write(&root.join("apps/api/.claude/skills/deploy/SKILL.md"), "---\nname: deploy\n---\nA copy");
        write(&root.join("apps/api/copy/.claude/skills/migrate/SKILL.md"), "Migrates");
        write(&root.join("apps/api/copy/deeper/.claude/skills/migrate/SKILL.md"), "Migrates");
        write(&root.join("node_modules/pkg/.claude/skills/no/SKILL.md"), "not listed");
        write(&root.join("target/x/.claude/skills/no/SKILL.md"), "not listed");
        write(&home.join(".claude/skills/synced/notes/SKILL.md"), "Take notes");
        let items = scan(&root, DEFAULT_FOLDER, &home);
        let got: Vec<(Kind, String, &str, &str)> = items.iter().map(|i| (i.kind, i.folders.join("/"), i.name.as_str(), i.description.as_str())).collect();
        let row = |k, f: &str, n, d| (k, f.to_owned(), n, d);
        assert_eq!(
            got,
            [
                row(Kind::Template, "", "a", "Alpha heading"),
                row(Kind::Template, "", "Beta", "second"),
                row(Kind::Skill, "", "deploy", "Ships it"),
                row(Kind::Skill, "apps/api", "migrate", "Migrates"),
                row(Kind::Skill, "team", "lint", "Lints"),
                row(Kind::Command, "", "review", "Review the diff"),
                row(Kind::Command, "git", "git:pr", "Open a PR"),
                row(Kind::PersonalSkill, "synced", "notes", "Take notes"),
            ]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn the_tree_groups_by_folder_then_by_a_shared_prefix() {
        let it = |name: &str, folders: &[&str]| Item { kind: Kind::Skill, path: PathBuf::new(), name: name.into(), description: String::new(), folders: folders.iter().map(|f| f.to_string()).collect() };
        let items = [it("speckit-plan", &[]), it("deploy", &[]), it("speckit-tasks", &[]), it("migrate", &["apps/api"]), it("solo-one", &[]), it("git:pr", &["git"])];
        let all: Vec<usize> = (0..items.len()).collect();
        let leaf = |item, label: &str| Node::Leaf { item, label: label.into() };
        let group = |label: &str, key: &str, children| Node::Group { label: label.into(), key: key.into(), children };
        assert_eq!(
            tree(&items, &all),
            [
                group("apps/api", "/apps/api", vec![leaf(3, "migrate")]),
                group("git", "/git", vec![leaf(5, "pr")]),
                leaf(1, "deploy"),
                leaf(4, "solo-one"),
                group("speckit", "/speckit-", vec![leaf(0, "plan"), leaf(2, "tasks")]),
            ]
        );
        assert_eq!(tree(&items, &all).iter().map(Node::count).sum::<usize>(), 6);
    }

    #[test]
    fn references_are_found_and_expanded_and_typed_takes_them_off() {
        assert_eq!(references("like @[Fix login] and @[Ship it], again @[Fix login]"), ["Fix login", "Ship it"]);
        assert!(references("mail me@example.com, @[ ] or @[unclosed").is_empty());
        let find = |name: &str| match name {
            "Fix login" => Ok(Reference { name: name.into(), transcript: "/p/s1.jsonl".into(), folder: Some("/w".into()), digest: "USER: fix\nCLAUDE: done\n".into() }),
            _ => Err("no such session".to_owned()),
        };
        let out = expand("redo @[Fix login]", find).unwrap();
        assert!(out.starts_with("redo @[Fix login]\n\n<referenced-sessions>"));
        assert!(out.contains("## @[Fix login]\nTranscript: /p/s1.jsonl\nWorked in: /w\nDigest of the conversation:\nUSER: fix\nCLAUDE: done\n"), "{out}");
        assert_eq!(typed(&out), "redo @[Fix login]");
        assert_eq!(expand("redo @[Nope]", find).unwrap_err(), "@[Nope]: no such session");
        assert_eq!(expand("plain", find).unwrap(), "plain");
        assert_eq!(typed("plain"), "plain");
        assert_eq!(display("redo @[Fix login] twice: @[Fix login]"), "redo `@[Fix login]` twice: `@[Fix login]`");
    }

    #[test]
    fn a_run_may_only_write_in_the_templates_folder() {
        let (root, dir) = (Path::new("/w/proj"), Path::new("/w/proj/.scope/templates"));
        let args = run_args(root, dir, true, None);
        let after = |k: &str| args.iter().position(|a| a == k).map(|i| args[i + 1].as_str());
        assert_eq!(after("--tools"), Some("Read,Grep,Glob,Write,Edit"));
        assert_eq!(after("--permission-mode"), Some("dontAsk"));
        assert_eq!(
            after("--allowedTools"),
            Some("Read,Grep,Glob,Edit(//w/proj/.scope/templates/**),Edit(//w/proj/.gitignore)")
        );
        let prompt = after("--append-system-prompt").unwrap();
        assert!(prompt.contains("Templates folder: /w/proj/.scope/templates"));
        assert!(prompt.contains("add the line `/.scope/templates/`"), "{prompt}");
        assert!(prompt.contains("{{target_file}}"));
        assert!(!run_args(root, Path::new("/elsewhere/t"), true, None).iter().any(|a| a.contains(".gitignore")), "a folder outside the root isn't ignored there");
        let no_ignore = run_args(root, dir, false, Some(&dir.join("x.md")));
        assert!(!no_ignore.iter().any(|a| a.contains(".gitignore")));
        assert!(no_ignore.iter().any(|a| a.contains("You are working on the template /w/proj/.scope/templates/x.md")));
    }

    #[test]
    fn the_written_template_is_the_md_file_a_write_names_in_the_folder() {
        let dir = Path::new("/w/t");
        let line = |name: &str, path: &str| serde_json::json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": name, "input": {"file_path": path}}]}});
        assert_eq!(written(&line("Write", "/w/t/a.md"), dir), Some(PathBuf::from("/w/t/a.md")));
        assert_eq!(written(&line("Edit", "/w/t/a.md"), dir), Some(PathBuf::from("/w/t/a.md")));
        assert_eq!(written(&line("Write", "/w/.gitignore"), dir), None);
        assert_eq!(written(&line("Read", "/w/t/a.md"), dir), None);
    }

    #[test]
    fn use_text_is_a_templates_body_or_a_skills_slash_command() {
        let t = Item { kind: Kind::Template, path: "/t.md".into(), name: "T".into(), description: String::new(), folders: Vec::new() };
        assert_eq!(use_text(&t, "---\nname: T\n---\n\nDo {{x}}.\n"), "Do {{x}}.");
        let s = Item { kind: Kind::Skill, name: "deploy".into(), ..t };
        assert_eq!(use_text(&s, "anything"), "/deploy ");
    }
}
