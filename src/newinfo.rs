//! What a new session will start with in its folder, shown before its first prompt: the model
//! and effort, the permission mode, and the Claude Code setup that applies there (instructions,
//! settings files, MCP servers, skills, agents, commands, hooks, plugins).
//!
//! Read from the same places Claude Code reads, with its precedence: the arguments scope passes
//! (`--model` from Settings, then the extra arguments), the environment, then
//! `.claude/settings.local.json`, `.claude/settings.json` and `~/.claude/settings.json`. It is
//! a preview: managed policies and anything Claude Code decides at start are not seen.
use eframe::egui;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A value and where it comes from ("scope settings", "~/.claude/settings.json", "default"…).
#[derive(Debug, Clone, PartialEq)]
pub struct Sourced {
    pub value: String,
    pub from: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewInfo {
    pub model: Option<Sourced>,
    pub effort: Option<Sourced>,
    pub permissions: Option<Sourced>,
    pub thinking: Option<Sourced>,
    /// Git branch of the folder, if it is in a repository.
    pub branch: Option<String>,
    /// CLAUDE.md files loaded, shortened with `~`.
    pub instructions: Vec<String>,
    /// Settings files that exist, most specific first.
    pub settings_files: Vec<String>,
    /// MCP servers by name, with their scope ("user", "project", "local").
    pub mcp: Vec<(String, String)>,
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub commands: Vec<String>,
    /// Hook events configured, e.g. "PreToolUse ×2".
    pub hooks: Vec<String>,
    pub plugins: Vec<String>,
    /// The extra arguments scope adds to every `claude -p`.
    pub extra_args: String,
}

/// Where scope's own Settings come into it.
pub struct Scope<'a> {
    pub model: &'a str,
    pub extra: &'a [String],
}

fn tilde(p: &Path, home: &Path) -> String {
    match p.strip_prefix(home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// The value after `flag` in `args` (`--flag v` or `--flag=v`).
fn flag(args: &[String], name: &str) -> Option<String> {
    let eq = format!("{name}=");
    args.iter().enumerate().find_map(|(i, a)| {
        if a == name {
            args.get(i + 1).cloned()
        } else {
            a.strip_prefix(&eq).map(str::to_owned)
        }
    })
}

/// Names in a folder: subfolders with a SKILL.md (skills) or `.md` files without the extension (agents,
/// commands), sorted.
fn names(dir: &Path, md: bool) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            if md {
                name.strip_suffix(".md").map(str::to_owned).filter(|_| p.is_file())
            } else {
                p.join("SKILL.md").is_file().then_some(name)
            }
        })
        .collect();
    v.sort();
    v
}

/// The branch checked out where `dir` is, read from `.git/HEAD` (a worktree's `.git` file
/// points to its own HEAD).
fn git_branch(dir: &Path) -> Option<String> {
    let git = dir.ancestors().map(|d| d.join(".git")).find(|g| g.exists())?;
    let head_dir = if git.is_file() {
        let text = std::fs::read_to_string(&git).ok()?;
        PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim())
    } else {
        git
    };
    let head = std::fs::read_to_string(head_dir.join("HEAD")).ok()?;
    let head = head.trim();
    Some(match head.strip_prefix("ref: refs/heads/") {
        Some(b) => b.to_owned(),
        None => format!("detached at {}", &head[..head.len().min(7)]),
    })
}

pub fn gather(dir: &Path, home: &Path, scope: &Scope, env: &dyn Fn(&str) -> Option<String>) -> NewInfo {
    let mut info = NewInfo { extra_args: scope.extra.join(" "), branch: git_branch(dir), ..Default::default() };
    let user_dir = home.join(".claude");
    // The project root: the repository's top, else the folder itself.
    let root = dir.ancestors().find(|d| d.join(".git").exists()).unwrap_or(dir).to_path_buf();

    // Settings files, most specific first (that is their precedence).
    let mut files: Vec<(PathBuf, Value)> = Vec::new();
    for p in [root.join(".claude/settings.local.json"), root.join(".claude/settings.json"), user_dir.join("settings.json")] {
        if let Some(v) = read_json(&p) {
            info.settings_files.push(tilde(&p, home));
            files.push((p, v));
        }
    }
    let from_files = |get: &dyn Fn(&Value) -> Option<String>| {
        files.iter().find_map(|(p, v)| get(v).map(|value| Sourced { value, from: tilde(p, home) }))
    };
    let s = |v: &Value| v.as_str().map(str::to_owned).filter(|s| !s.trim().is_empty());

    info.model = if !scope.model.trim().is_empty() {
        Some(Sourced { value: scope.model.trim().to_owned(), from: "scope settings".into() })
    } else if let Some(m) = flag(scope.extra, "--model") {
        Some(Sourced { value: m, from: "extra arguments".into() })
    } else if let Some(m) = env("ANTHROPIC_MODEL").filter(|m| !m.is_empty()) {
        Some(Sourced { value: m, from: "ANTHROPIC_MODEL".into() })
    } else {
        from_files(&|v| v.get("model").and_then(s))
    };
    info.effort = flag(scope.extra, "--effort")
        .map(|e| Sourced { value: e, from: "extra arguments".into() })
        .or_else(|| env("CLAUDE_CODE_EFFORT_LEVEL").filter(|e| !e.is_empty()).map(|e| Sourced { value: e, from: "CLAUDE_CODE_EFFORT_LEVEL".into() }))
        .or_else(|| from_files(&|v| v.get("effortLevel").and_then(s)));
    info.permissions = if scope.extra.iter().any(|a| a == "--dangerously-skip-permissions") {
        Some(Sourced { value: "bypassPermissions".into(), from: "extra arguments".into() })
    } else if let Some(m) = flag(scope.extra, "--permission-mode") {
        Some(Sourced { value: m, from: "extra arguments".into() })
    } else {
        from_files(&|v| v.pointer("/permissions/defaultMode").and_then(s))
    };
    info.thinking = from_files(&|v| v.get("alwaysThinkingEnabled").and_then(Value::as_bool).map(|b| if b { "always on" } else { "off" }.to_owned()));

    // Instructions: the user's, then every CLAUDE.md from the filesystem root down to the folder.
    let mut md = vec![user_dir.join("CLAUDE.md")];
    let mut up: Vec<&Path> = dir.ancestors().collect();
    up.reverse();
    for d in up {
        for name in ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"] {
            md.push(d.join(name));
        }
    }
    let mut seen = std::collections::HashSet::new();
    info.instructions = md.into_iter().filter(|p| p.is_file() && seen.insert(p.clone())).map(|p| tilde(&p, home)).collect();

    // MCP servers: user (~/.claude.json and user settings), local (~/.claude.json's entry for
    // this project), project (.mcp.json).
    let mut mcp: Vec<(String, String)> = Vec::new();
    let mut add = |v: Option<&Value>, scope: &str| {
        for name in v.and_then(Value::as_object).into_iter().flat_map(|o| o.keys()) {
            if !mcp.iter().any(|(n, _)| n == name) {
                mcp.push((name.clone(), scope.to_owned()));
            }
        }
    };
    let state = read_json(&home.join(".claude.json"));
    let project_state = state.as_ref().and_then(|s| s.get("projects")).and_then(|p| p.get(root.to_string_lossy().as_ref()));
    add(project_state.and_then(|p| p.get("mcpServers")), "local");
    add(read_json(&root.join(".mcp.json")).as_ref().and_then(|v| v.get("mcpServers")), "project");
    add(state.as_ref().and_then(|s| s.get("mcpServers")), "user");
    for (p, v) in &files {
        add(v.get("mcpServers"), if p.starts_with(&user_dir) { "user" } else { "project" });
    }
    info.mcp = mcp;

    // Skills, agents and commands: the project's, then the user's.
    let both = |sub: &str, md: bool| {
        let mut v = names(&root.join(".claude").join(sub), md);
        for n in names(&user_dir.join(sub), md) {
            if !v.contains(&n) {
                v.push(n);
            }
        }
        v
    };
    info.skills = both("skills", false);
    info.agents = both("agents", true);
    info.commands = both("commands", true);

    // Hooks: events and how many hooks each has, over every settings file.
    let mut hooks: Vec<(String, usize)> = Vec::new();
    for (_, v) in &files {
        for (event, matchers) in v.get("hooks").and_then(Value::as_object).into_iter().flatten() {
            let n: usize = matchers.as_array().into_iter().flatten().map(|m| m.get("hooks").and_then(Value::as_array).map_or(1, Vec::len)).sum();
            match hooks.iter_mut().find(|(e, _)| e == event) {
                Some((_, c)) => *c += n,
                None => hooks.push((event.clone(), n)),
            }
        }
    }
    info.hooks = hooks.into_iter().map(|(e, n)| if n > 1 { format!("{e} ×{n}") } else { e }).collect();

    // Plugins switched on in any settings file.
    for (_, v) in &files {
        for (name, on) in v.get("enabledPlugins").and_then(Value::as_object).into_iter().flatten() {
            if on.as_bool() == Some(true) && !info.plugins.contains(name) {
                info.plugins.push(name.clone());
            }
        }
    }
    info
}

/// Shows `info` for a new session in `dir`, centred, with `status` under it.
/// Model aliases, effort levels and permission modes offered on the new session page.
pub const MODELS: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
pub const PERMISSIONS: [&str; 4] = ["default", "acceptEdits", "plan", "bypassPermissions"];

/// A dropdown for `value` among `options` (value, label); empty = `default`, what applies
/// when nothing is picked.
fn picker(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[(String, String)], default: &str, enabled: bool) {
    let shown = match options.iter().find(|(v, _)| v == value) {
        Some((_, label)) if !value.is_empty() => label.clone(),
        _ => default.to_owned(),
    };
    ui.add_enabled_ui(enabled, |ui| {
        ui.push_id(id, |ui| {
            ui.menu_button(egui::RichText::new(format!("{shown} ⏷")).monospace(), |ui| {
                for (v, label) in std::iter::once((String::new(), default.to_owned())).chain(options.iter().cloned()) {
                    if ui.selectable_label(*value == v, label).clicked() {
                        *value = v;
                        ui.close();
                    }
                }
            });
        });
    });
}

fn same(v: &[&str]) -> Vec<(String, String)> {
    v.iter().map(|o| ((*o).to_owned(), (*o).to_owned())).collect()
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Shows `info` for a new session in `dir`, centred, with `status` under it. What can be
/// changed for this session has a dropdown (`choice`), usable while `editable`.
pub fn show(ui: &mut egui::Ui, dir: &Path, info: &NewInfo, choice: &mut crate::runner::Choice, editable: bool, status: &str) {
    use egui::RichText;
    let default = |o: &Option<Sourced>, d: &str| match o {
        Some(s) => (s.value.clone(), s.from.clone()),
        None => (d.to_owned(), "Claude Code's default".to_owned()),
    };
    let list = |v: &[String]| if v.is_empty() { "none".to_owned() } else { v.join(", ") };
    egui::ScrollArea::vertical().id_salt("new-info").auto_shrink(false).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new(format!("New session in {}", dir.display())).strong().size(15.0));
            if let Some(b) = &info.branch {
                ui.label(RichText::new(format!("on {b}")).weak());
            }
            ui.add_space(14.0);
            let width = ui.available_width().min(640.0);
            ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(12.0).corner_radius(6.0).show(ui, |ui| {
                    ui.set_width(width - 24.0);
                    egui::Grid::new("new-info-grid").num_columns(2).spacing([16.0, 6.0]).striped(false).show(ui, |ui| {
                        let row = |ui: &mut egui::Ui, key: &str, value: String, from: Option<String>| {
                            ui.label(RichText::new(key).weak());
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(value).monospace());
                                if let Some(f) = from {
                                    ui.label(RichText::new(format!("({f})")).weak().small());
                                }
                            });
                            ui.end_row();
                        };
                        // A dropdown, defaulting to what applies (`shown`), with `note` beside
                        // it until something else is picked.
                        let pick = |ui: &mut egui::Ui, key: &str, value: &mut String, options: &[(String, String)], shown: String, note: String| {
                            ui.label(RichText::new(key).weak());
                            ui.horizontal_wrapped(|ui| {
                                picker(ui, key, value, options, &format!("{shown} (as set)"), editable);
                                let note = if value.is_empty() { note } else { "(this session; later prompts keep it)".to_owned() };
                                ui.label(RichText::new(note).weak().small());
                            });
                            ui.end_row();
                        };
                        let sourced = |o: &Option<Sourced>, d: &str| {
                            let (v, from) = default(o, d);
                            (v, format!("({from})"))
                        };
                        let (v, n) = sourced(&info.model, "default");
                        pick(ui, "Model", &mut choice.model, &same(&MODELS), v, n);
                        let (v, n) = sourced(&info.effort, "default");
                        pick(ui, "Effort", &mut choice.effort, &same(&EFFORTS), v, n);
                        let (v, n) = sourced(&info.permissions, "default (asks)");
                        pick(ui, "Permissions", &mut choice.permissions, &same(&PERMISSIONS), v, n);
                        let (v, n) = sourced(&info.thinking, "off");
                        let think = vec![("on".to_owned(), "thinking on".to_owned()), ("off".to_owned(), "thinking off".to_owned())];
                        pick(ui, "Thinking", &mut choice.thinking, &think, format!("thinking {v}"), n);
                        if !info.extra_args.is_empty() {
                            row(ui, "Extra arguments", info.extra_args.clone(), None);
                        }
                        row(ui, "Instructions", if info.instructions.is_empty() { "no CLAUDE.md".into() } else { info.instructions.join("\n") }, None);
                        let sources = [("user", "your settings only"), ("project,local", "the project's only"), ("user,project", "yours and the project's (no local)")];
                        let sources: Vec<(String, String)> = sources.iter().map(|(v, l)| ((*v).to_owned(), (*l).to_owned())).collect();
                        let files = if info.settings_files.is_empty() { "none found".into() } else { info.settings_files.join("\n") };
                        pick(ui, "Settings files", &mut choice.sources, &sources, "all of them".into(), files);
                        let off = |what: &str| vec![("off".to_owned(), format!("no {what}"))];
                        if info.mcp.is_empty() {
                            row(ui, "MCP servers", "none".into(), None);
                        } else {
                            let mcp: Vec<String> = info.mcp.iter().map(|(n, s)| format!("{n} ({s})")).collect();
                            let none = vec![("none".to_owned(), "no MCP servers".to_owned())];
                            pick(ui, "MCP servers", &mut choice.mcp, &none, count(mcp.len(), "server", "servers"), list(&mcp));
                        }
                        // Skills and commands go on and off together (one flag).
                        if info.skills.is_empty() && info.commands.is_empty() {
                            row(ui, "Skills", "none".into(), None);
                            row(ui, "Commands", "none".into(), None);
                        } else {
                            pick(ui, "Skills", &mut choice.slash, &off("skills or commands"), count(info.skills.len(), "skill", "skills"), list(&info.skills));
                            pick(ui, "Commands", &mut choice.slash, &off("skills or commands"), count(info.commands.len(), "command", "commands"), list(&info.commands));
                        }
                        if info.agents.is_empty() {
                            row(ui, "Agent", "main (no custom agents)".into(), None);
                        } else {
                            pick(ui, "Agent", &mut choice.agent, &same(&info.agents.iter().map(String::as_str).collect::<Vec<_>>()), "main".into(), format!("available: {}", list(&info.agents)));
                        }
                        if info.hooks.is_empty() {
                            row(ui, "Hooks", "none".into(), None);
                        } else {
                            pick(ui, "Hooks", &mut choice.hooks, &off("hooks"), count(info.hooks.len(), "hook event", "hook events"), list(&info.hooks));
                        }
                        if !info.plugins.is_empty() {
                            let all = vec![(info.plugins.join(","), "no plugins".to_owned())];
                            pick(ui, "Plugins", &mut choice.plugins, &all, count(info.plugins.len(), "plugin", "plugins"), list(&info.plugins));
                        }
                    });
                });
            });
            ui.add_space(14.0);
            ui.label(status);
            ui.label(RichText::new("Read from the folder's and your Claude Code settings; plugins and managed policies may add more.").weak().small());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("scope-newinfo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn project_settings_win_over_the_users_and_scope_wins_over_both() {
        let t = tmp("prec");
        let home = t.join("home");
        let proj = t.join("proj");
        std::fs::create_dir_all(proj.join(".git")).unwrap();
        write(&proj.join(".git/HEAD"), "ref: refs/heads/feature/x\n");
        write(&home.join(".claude/settings.json"), r#"{"model":"opus","effortLevel":"high","permissions":{"defaultMode":"bypassPermissions"},"hooks":{"PreToolUse":[{"hooks":[{},{}]}]},"mcpServers":{"jira":{}}}"#);
        write(&proj.join(".claude/settings.json"), r#"{"model":"sonnet","enabledPlugins":{"p@m":true,"off@m":false}}"#);
        let sub = proj.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let none: Vec<String> = Vec::new();
        let i = gather(&sub, &home, &Scope { model: "", extra: &none }, &no_env);
        assert_eq!(i.model.as_ref().unwrap().value, "sonnet");
        assert_eq!(i.effort.as_ref().unwrap().value, "high");
        assert_eq!(i.effort.as_ref().unwrap().from, "~/.claude/settings.json");
        assert_eq!(i.permissions.as_ref().unwrap().value, "bypassPermissions");
        assert_eq!(i.branch.as_deref(), Some("feature/x"));
        assert_eq!(i.hooks, ["PreToolUse ×2"]);
        assert_eq!(i.mcp, [("jira".to_owned(), "user".to_owned())]);
        assert_eq!(i.plugins, ["p@m"]);

        let extra: Vec<String> = ["--permission-mode", "acceptEdits", "--effort=low"].map(String::from).to_vec();
        let i = gather(&sub, &home, &Scope { model: "fable", extra: &extra }, &|k| (k == "ANTHROPIC_MODEL").then(|| "x".into()));
        assert_eq!(i.model.unwrap(), Sourced { value: "fable".into(), from: "scope settings".into() });
        assert_eq!(i.effort.unwrap().value, "low");
        assert_eq!(i.permissions.unwrap().value, "acceptEdits");
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn lists_instructions_mcp_skills_agents_and_commands() {
        let t = tmp("lists");
        let home = t.join("home");
        let proj = t.join("proj");
        std::fs::create_dir_all(proj.join(".git")).unwrap();
        write(&home.join(".claude/CLAUDE.md"), "me");
        write(&proj.join("CLAUDE.md"), "proj");
        write(&proj.join(".mcp.json"), r#"{"mcpServers":{"db":{}}}"#);
        write(&home.join(".claude.json"), &format!(r#"{{"mcpServers":{{"web":{{}}}},"projects":{{"{}":{{"mcpServers":{{"mine":{{}}}}}}}}}}"#, proj.display()));
        write(&proj.join(".claude/skills/deploy/SKILL.md"), "");
        write(&home.join(".claude/skills/notes/SKILL.md"), "");
        write(&proj.join(".claude/agents/reviewer.md"), "");
        write(&home.join(".claude/commands/ship.md"), "");
        let none: Vec<String> = Vec::new();
        let i = gather(&proj, &home, &Scope { model: "", extra: &none }, &no_env);
        assert_eq!(i.instructions[0], "~/.claude/CLAUDE.md");
        assert!(i.instructions[1].ends_with("proj/CLAUDE.md"));
        assert_eq!(i.mcp.iter().map(|(n, s)| format!("{n}:{s}")).collect::<Vec<_>>(), ["mine:local", "db:project", "web:user"]);
        assert_eq!(i.skills, ["deploy", "notes"]);
        assert_eq!(i.agents, ["reviewer"]);
        assert_eq!(i.commands, ["ship"]);
        assert!(i.model.is_none() && i.effort.is_none(), "nothing set: Claude Code's defaults");
        let _ = std::fs::remove_dir_all(&t);
    }
}
