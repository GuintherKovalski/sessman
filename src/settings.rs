use eframe::egui::{self, RichText, ThemePreference};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// scope's own folder: `~/.config/scope`, or `$SCOPE_CONFIG` (used to run a test copy
/// of the app next to the real one).
pub fn config_dir() -> PathBuf {
    match std::env::var_os("SCOPE_CONFIG") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/scope"),
    }
}

/// Before the app was renamed scope its folder was `~/.config/sessman`: copied over (not moved,
/// so a sessman still running keeps its own) the first time scope starts without one.
pub fn adopt_old_config() {
    let new = config_dir();
    let old = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/sessman");
    if std::env::var_os("SCOPE_CONFIG").is_none() && !new.exists() && old.is_dir() {
        let _ = std::process::Command::new("cp").arg("-R").arg(&old).arg(&new).status();
    }
}

pub fn path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Everything on the Settings page. Missing keys take their default, so older files (which
/// only had `theme`) still load.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "dark", "light" or "system".
    pub theme: String,
    pub zoom: f32,
    pub mono_size: f32,
    /// Send with Cmd/Ctrl+Enter; plain Enter then adds a new line.
    pub send_with_modifier: bool,
    /// Lines the prompt input grows to before it scrolls.
    pub input_max_lines: usize,
    /// Keep what is typed in the input per session, when switching sessions.
    pub drafts_per_session: bool,
    pub show_tool_lines: bool,
    pub line_numbers: bool,
    pub wrap_file_lines: bool,
    pub tab_width: usize,
    pub markdown_rendered: bool,
    pub claude_path: String,
    /// Model for prompts; empty = Claude Code's default.
    pub model: String,
    /// Model for search, goals and refine.
    pub helper_model: String,
    /// Extra arguments for `claude -p`, split on spaces.
    pub extra_args: String,
    /// Where the New session folder picker starts before any folder has been chosen (after that,
    /// the last one chosen); empty = the open session's folder.
    pub default_folder: String,
    pub confirm_delete_workspace: bool,
    pub confirm_delete_message: bool,
    pub restore_state: bool,
    pub notify_when_done: bool,
    /// After a run ends, the helper model reads the session's last messages and tags it done,
    /// waiting for you or blocked (verdict.rs).
    pub tag_finished: bool,
    /// Jira site the overview's ticket keys link to, e.g. https://acme.atlassian.net.
    pub jira_url: String,
    /// Harness: the instructions each helper starts from (Settings → Harness). Empty = default.
    pub orchestration_harness: String,
    pub question_harness: String,
    pub search_harness: String,
    /// Model for workspace orchestration; empty = the helper model.
    pub orchestration_model: String,
    /// Minutes between automatic orchestration updates while there is new activity; 0 = only
    /// when asked.
    pub orchestration_every: u32,
    /// Hours without activity after which the overview shows a session in red and offers to
    /// finish it; 0 = never.
    pub stale_after_hours: u32,
    /// Look for a newer scope on GitHub at start and twice a day (Settings → About).
    pub check_for_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            zoom: 1.0,
            mono_size: 15.0,
            send_with_modifier: false,
            input_max_lines: 10,
            drafts_per_session: true,
            show_tool_lines: true,
            line_numbers: true,
            wrap_file_lines: false,
            tab_width: 4,
            markdown_rendered: true,
            claude_path: "claude".into(),
            model: String::new(),
            helper_model: "haiku".into(),
            extra_args: String::new(),
            default_folder: String::new(),
            confirm_delete_workspace: true,
            confirm_delete_message: true,
            restore_state: true,
            notify_when_done: false,
            tag_finished: true,
            jira_url: String::new(),
            orchestration_harness: crate::orchestration::DEFAULT_HARNESS.into(),
            question_harness: crate::goal::QUESTION_HARNESS.into(),
            search_harness: crate::search::DEFAULT_HARNESS.into(),
            orchestration_model: String::new(),
            orchestration_every: 10,
            stale_after_hours: 24,
            check_for_updates: true,
        }
    }
}

impl Settings {
    /// The harness text, or `default` when it was emptied.
    pub fn harness<'a>(text: &'a str, default: &'a str) -> &'a str {
        if text.trim().is_empty() { default } else { text }
    }

    pub fn theme(&self) -> ThemePreference {
        match self.theme.as_str() {
            "dark" => ThemePreference::Dark,
            "light" => ThemePreference::Light,
            _ => ThemePreference::System,
        }
    }

    /// Zoom, font size and theme, applied to egui.
    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_theme(self.theme());
        ctx.set_zoom_factor(self.zoom.clamp(0.5, 3.0));
        ctx.all_styles_mut(|style| {
            if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Monospace) {
                font.size = self.mono_size.clamp(8.0, 32.0);
            }
        });
    }
}

/// Keyboard shortcuts, listed on the Settings page.
pub fn shortcuts(s: &Settings) -> Vec<(&'static str, String)> {
    let cmd = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
    let (send, newline, refine) = if s.send_with_modifier {
        (format!("{cmd}+Enter"), "Enter / Shift+Enter".to_owned(), format!("{cmd}+Shift+Enter"))
    } else {
        ("Enter".to_owned(), "Shift+Enter".to_owned(), format!("{cmd}+Enter"))
    };
    vec![
        ("Send the prompt (queued while Claude works)", send),
        ("New line in the prompt", newline),
        ("Refine the prompt", refine),
        ("Refine the prompt, wherever the focus is", format!("{cmd}+R")),
        ("Dictate: start, or stop and transcribe (Enter stops too)", format!("{cmd}+D")),
        ("Stop Claude", "Ctrl+C".to_owned()),
        ("Previous / next prompt (or pull back a queued one)", "Up / Down".to_owned()),
        ("Open settings", format!("{cmd}+,")),
        ("Find in the conversation or open file (Enter / Shift+Enter: next / previous)", format!("Ctrl+F / {cmd}+F")),
        ("Zoom in / out / reset", format!("{cmd}+Plus / {cmd}+Minus / {cmd}+0")),
        ("Undo refine", format!("{cmd}+Z")),
        ("Close a file tab", "Middle-click".to_owned()),
        ("Search sessions with Haiku", "Enter in the search field".to_owned()),
    ]
}

/// What the Settings page shows besides the settings themselves.
pub struct About<'a> {
    pub status_line: &'a str,
    pub updater: &'a mut crate::update::Updater,
}

/// True if `query` matches a setting's label or section (case-insensitive, every word).
pub fn matches(query: &str, section: &str, label: &str) -> bool {
    let hay = format!("{section} {label}").to_lowercase();
    query.split_whitespace().all(|w| hay.contains(&w.to_lowercase()))
}

/// A group of settings, drawn as a card like a scope's: its title, what it is about, and a
/// stripe in its color.
struct Group {
    title: &'static str,
    about: &'static str,
    color: egui::Color32,
}

const APPEARANCE: Group = Group { title: "Appearance", about: "How scope looks", color: egui::Color32::from_rgb(170, 120, 220) };
const PROMPT: Group = Group { title: "Prompt input", about: "The field at the bottom of a session", color: egui::Color32::from_rgb(90, 155, 230) };
const READING: Group = Group { title: "Conversation and files", about: "What the middle area shows", color: egui::Color32::from_rgb(80, 185, 170) };
const CLAUDE: Group = Group { title: "Claude Code", about: "How scope runs claude for your prompts", color: egui::Color32::from_rgb(225, 145, 85) };
const SESSIONS: Group = Group { title: "Sessions and scopes", about: "The session list, its tags, and the scopes overview", color: egui::Color32::from_rgb(105, 190, 115) };
const HELPERS: Group = Group { title: "AI helpers and harnesses", about: "What runs, and what it is told, when you ask something", color: egui::Color32::from_rgb(215, 175, 65) };
const SHORTCUTS: Group = Group { title: "Keyboard shortcuts", about: "", color: egui::Color32::from_rgb(140, 140, 150) };
const ABOUT: Group = Group { title: "About", about: "Version, updates, config folder", color: egui::Color32::from_rgb(140, 140, 150) };

/// The Settings page. Returns true when a setting changed (to save and apply it).
pub fn ui(ui: &mut egui::Ui, s: &mut Settings, filter: &mut String, about: &mut About) -> bool {
    let before = s.clone();
    let defaults = Settings::default();
    ui.horizontal(|ui| {
        ui.heading("Settings");
        ui.add_space(12.0);
        ui.add(egui::TextEdit::singleline(filter).hint_text("🔍 Search settings").desired_width(260.0));
    });
    ui.label(RichText::new(format!("Saved to {}", path().display())).weak().small());
    ui.add_space(4.0);
    let q = filter.trim().to_owned();
    egui::ScrollArea::vertical().id_salt("settings").auto_shrink(false).show(ui, |ui| {
        let mut page = Page { ui, q: &q, context: String::new(), probe: false, hits: 0, label_width: 300.0 };

        page.group(&APPEARANCE, |page| {
            page.row("Theme", "Dark, light, or follow the system", s.theme != defaults.theme, |ui, reset| {
                if reset {
                    s.theme = defaults.theme.clone();
                }
                for (v, label) in [("dark", "🌙 Dark"), ("light", "☀ Light"), ("system", "💻 System")] {
                    ui.selectable_value(&mut s.theme, v.to_owned(), label);
                }
            });
            page.row("Zoom", "Also Cmd+Plus, Cmd+Minus and Cmd+0", (s.zoom - defaults.zoom).abs() > 0.001, |ui, reset| {
                if reset {
                    s.zoom = defaults.zoom;
                }
                if ui.small_button("−").on_hover_text("Zoom out").clicked() {
                    s.zoom = ((s.zoom - 0.1) * 10.0).round() / 10.0;
                }
                ui.label(RichText::new(format!("{:.0}%", s.zoom * 100.0)).monospace());
                if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                    s.zoom = ((s.zoom + 0.1) * 10.0).round() / 10.0;
                }
                s.zoom = s.zoom.clamp(0.5, 3.0);
            });
            page.row("Monospace font size", "Prompt input, code and files", s.mono_size != defaults.mono_size, |ui, reset| {
                if reset {
                    s.mono_size = defaults.mono_size;
                }
                ui.add(egui::Slider::new(&mut s.mono_size, 9.0..=20.0).step_by(0.5));
            });
        });

        page.group(&PROMPT, |page| {
            page.row("Send with Cmd/Ctrl+Enter", "Off: Enter sends. On: Enter adds a new line", s.send_with_modifier != defaults.send_with_modifier, |ui, reset| {
                if reset {
                    s.send_with_modifier = defaults.send_with_modifier;
                }
                ui.checkbox(&mut s.send_with_modifier, "");
            });
            page.row("Maximum height (lines)", "The input grows up to this, then scrolls", s.input_max_lines != defaults.input_max_lines, |ui, reset| {
                if reset {
                    s.input_max_lines = defaults.input_max_lines;
                }
                ui.add(egui::Slider::new(&mut s.input_max_lines, 2..=40));
            });
            page.row("Keep a draft per session", "What you type stays with its session when you switch", s.drafts_per_session != defaults.drafts_per_session, |ui, reset| {
                if reset {
                    s.drafts_per_session = defaults.drafts_per_session;
                }
                ui.checkbox(&mut s.drafts_per_session, "");
            });
        });

        page.group(&READING, |page| {
            page.row("Show tool lines", "The dimmed ⚙ Read · Edit lines between messages", s.show_tool_lines != defaults.show_tool_lines, |ui, reset| {
                if reset {
                    s.show_tool_lines = defaults.show_tool_lines;
                }
                ui.checkbox(&mut s.show_tool_lines, "");
            });
            page.row("Line numbers", "In open files", s.line_numbers != defaults.line_numbers, |ui, reset| {
                if reset {
                    s.line_numbers = defaults.line_numbers;
                }
                ui.checkbox(&mut s.line_numbers, "");
            });
            page.row("Wrap long lines", "In open files; off: scroll sideways", s.wrap_file_lines != defaults.wrap_file_lines, |ui, reset| {
                if reset {
                    s.wrap_file_lines = defaults.wrap_file_lines;
                }
                ui.checkbox(&mut s.wrap_file_lines, "");
            });
            page.row("Tab width", "Spaces shown for a tab character", s.tab_width != defaults.tab_width, |ui, reset| {
                if reset {
                    s.tab_width = defaults.tab_width;
                }
                ui.add(egui::Slider::new(&mut s.tab_width, 1..=8));
            });
            page.row("Open Markdown rendered", "Off: .md files open as source", s.markdown_rendered != defaults.markdown_rendered, |ui, reset| {
                if reset {
                    s.markdown_rendered = defaults.markdown_rendered;
                }
                ui.checkbox(&mut s.markdown_rendered, "");
            });
        });

        page.group(&CLAUDE, |page| {
            page.row("claude command", "Path to the Claude Code CLI", s.claude_path != defaults.claude_path, |ui, reset| {
                if reset {
                    s.claude_path = defaults.claude_path.clone();
                }
                ui.add(egui::TextEdit::singleline(&mut s.claude_path).desired_width(260.0));
            });
            page.row("Model for prompts", "Default = Claude Code's default", s.model != defaults.model, |ui, reset| {
                if reset {
                    s.model = defaults.model.clone();
                }
                for (v, label) in [("", "Default"), ("opus", "Opus"), ("fable", "Fable")] {
                    ui.selectable_value(&mut s.model, v.to_owned(), label);
                }
            });
            page.row("Extra arguments", "Added to every claude -p, e.g. --permission-mode acceptEdits", s.extra_args != defaults.extra_args, |ui, reset| {
                if reset {
                    s.extra_args = defaults.extra_args.clone();
                }
                ui.add(egui::TextEdit::singleline(&mut s.extra_args).desired_width(260.0));
            });
            page.row("Default folder for new sessions", "Where the folder picker starts before any folder has been chosen; empty = the open session's folder", s.default_folder != defaults.default_folder, |ui, reset| {
                if reset {
                    s.default_folder = defaults.default_folder.clone();
                }
                ui.add(egui::TextEdit::singleline(&mut s.default_folder).desired_width(260.0));
            });
        });

        page.group(&SESSIONS, |page| {
            page.row("Tag finished sessions", "After a run ends, the helper model reads the last messages and tags the session done, needs you or blocked", s.tag_finished != defaults.tag_finished, |ui, reset| {
                if reset {
                    s.tag_finished = defaults.tag_finished;
                }
                ui.checkbox(&mut s.tag_finished, "");
            });
            page.row("Notify when a run finishes", "A system notification, while scope is in the background", s.notify_when_done != defaults.notify_when_done, |ui, reset| {
                if reset {
                    s.notify_when_done = defaults.notify_when_done;
                }
                ui.checkbox(&mut s.notify_when_done, "");
            });
            page.row("Restore the previous state at start", "Open session, tabs, drafts and queues", s.restore_state != defaults.restore_state, |ui, reset| {
                if reset {
                    s.restore_state = defaults.restore_state;
                }
                ui.checkbox(&mut s.restore_state, "");
            });
            page.row("Confirm before deleting a scope", "The sessions themselves are kept", s.confirm_delete_workspace != defaults.confirm_delete_workspace, |ui, reset| {
                if reset {
                    s.confirm_delete_workspace = defaults.confirm_delete_workspace;
                }
                ui.checkbox(&mut s.confirm_delete_workspace, "");
            });
            page.row("Confirm before deleting a message", "From a session's context, with 🗑 on the message", s.confirm_delete_message != defaults.confirm_delete_message, |ui, reset| {
                if reset {
                    s.confirm_delete_message = defaults.confirm_delete_message;
                }
                ui.checkbox(&mut s.confirm_delete_message, "");
            });
            page.row("Offer to finish sessions idle for (hours)", "In the scopes overview: Finish session takes them out of the scope; 0 = never", s.stale_after_hours != defaults.stale_after_hours, |ui, reset| {
                if reset {
                    s.stale_after_hours = defaults.stale_after_hours;
                }
                ui.add(egui::Slider::new(&mut s.stale_after_hours, 0..=168));
            });
            page.row("Jira URL", "Ticket keys in the scopes overview link here, e.g. https://acme.atlassian.net", s.jira_url != defaults.jira_url, |ui, reset| {
                if reset {
                    s.jira_url = defaults.jira_url.clone();
                }
                ui.add(egui::TextEdit::singleline(&mut s.jira_url).hint_text("https://…atlassian.net").desired_width(260.0));
            });
        });

        page.group(&HELPERS, |page| {
            page.note(
                "Each place below asks a model a one-off question (claude -p). The harness is \
                 your part of what it is told: edit it here, ↺ or empty it to get the default \
                 back. scope adds the rest (the data, and the reply format it reads), shown \
                 under Full prompt.",
            );
            page.row("Helper model", "Answers the search, the built-in helpers, and the orchestrator unless set there", s.helper_model != defaults.helper_model, |ui, reset| {
                if reset {
                    s.helper_model = defaults.helper_model.clone();
                }
                ui.add(egui::TextEdit::singleline(&mut s.helper_model).desired_width(160.0));
            });
            let helper = model_name(&s.helper_model, "haiku");
            let orchestrator = if s.orchestration_model.trim().is_empty() { format!("{helper} (the helper model)") } else { s.orchestration_model.trim().to_owned() };

            page.card("🔍 Search bar", "Bottom of the session list: press Enter in the search field", |page| {
                page.facts(&[
                    ("Answered by", format!("{helper} (the helper model)")),
                    ("It sees", "the name, goal and first prompts of the 40 newest sessions in the open tab".into()),
                    ("It gives back", "up to 8 matching sessions, best first, with a reason each".into()),
                ]);
                page.harness("Harness", s.search_harness != defaults.search_harness, &mut s.search_harness, &defaults.search_harness);
                page.preview("Full prompt", &preview::search(&s.search_harness));
            });

            page.card("◆ Orchestrator", "A scope's orchestration page: its report, and the Ask field at the bottom (also scope ask)", |page| {
                page.facts(&[
                    ("Answered by", orchestrator.clone()),
                    ("The report sees", "its previous report, then each session's status, goal and new activity".into()),
                    ("A question sees", "the report, the last 3 questions, and a short digest of each session".into()),
                    ("It can act", "rename sessions, reorder the cards, start sessions: only when you ask it to".into()),
                ]);
                page.harness("Harness (report and questions)", s.orchestration_harness != defaults.orchestration_harness, &mut s.orchestration_harness, &defaults.orchestration_harness);
                page.row("Orchestration model", "e.g. sonnet, opus; empty = the helper model", s.orchestration_model != defaults.orchestration_model, |ui, reset| {
                    if reset {
                        s.orchestration_model = defaults.orchestration_model.clone();
                    }
                    ui.add(egui::TextEdit::singleline(&mut s.orchestration_model).hint_text("helper model").desired_width(160.0));
                });
                page.row("Update the report every (minutes)", "While its sessions have new activity and its page is open; 0 = only when asked", s.orchestration_every != defaults.orchestration_every, |ui, reset| {
                    if reset {
                        s.orchestration_every = defaults.orchestration_every;
                    }
                    ui.add(egui::Slider::new(&mut s.orchestration_every, 0..=120));
                });
                page.preview("Full prompt: a question", &preview::orchestrator_question(&s.orchestration_harness));
                page.preview("Full prompt: a report update", &preview::orchestrator_report(&s.orchestration_harness));
            });

            page.card("💬 Ask about this goal", "Right panel: Info → Ask about this goal", |page| {
                page.facts(&[
                    ("Answered by", "Claude Code's default model, in a throwaway copy of the session (like /btw)".into()),
                    ("It sees", "the whole conversation, plus the session's goal and checklist".into()),
                    ("It changes", "nothing: the copy isn't kept, and the session gets no new message".into()),
                ]);
                page.harness("Harness", s.question_harness != defaults.question_harness, &mut s.question_harness, &defaults.question_harness);
                page.preview("Full prompt", &preview::question(&s.question_harness));
            });

            page.card("Built-in helpers", "Fixed: scope reads their replies in these exact formats", |page| {
                let fixed = [
                    ("Names a new session", "after its first reply", preview::name()),
                    ("Goal and checklist", "Info → Summarize", preview::goal()),
                    ("Tags a finished session", "after a run ends, with Tag finished sessions on", preview::tag()),
                    ("Refine", "Cmd+Enter in the prompt input", preview::refine()),
                    ("Scopes overview cards", "All sessions, when a scope has new activity", preview::overview()),
                ];
                for (what, when, text) in fixed {
                    page.preview_row(what, &format!("{when} · {helper}"), &text);
                }
                page.preview_row("Every session you run", "added to its system prompt, so Claude can manage scope's sessions", &preview::every_session());
            });
        });

        page.group(&SHORTCUTS, |page| {
            for (what, keys) in shortcuts(s) {
                page.row(what, "", false, |ui, _| {
                    ui.label(RichText::new(&keys).monospace());
                });
            }
        });

        page.group(&ABOUT, |page| {
            page.row("Version", "", false, |ui, _| {
                ui.label(env!("CARGO_PKG_VERSION"));
                update_status(ui, about.updater);
            });
            page.row("Check for updates automatically", "At start and twice a day, on GitHub; installing always waits for you", s.check_for_updates != defaults.check_for_updates, |ui, reset| {
                if reset {
                    s.check_for_updates = defaults.check_for_updates;
                }
                ui.checkbox(&mut s.check_for_updates, "");
            });
            page.row("Config folder", "", false, |ui, _| {
                let dir = config_dir();
                ui.label(RichText::new(dir.display().to_string()).monospace());
                if ui.small_button("Open in Finder").clicked() {
                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                }
            });
            page.row("Status line (usage limits)", "", false, |ui, _| {
                ui.label(about.status_line);
            });
        });
        if page.hits == 0 {
            page.ui.label("No setting matches.");
        }
    });
    *s != before
}

/// A model setting as it is used: `fallback` when empty.
fn model_name(model: &str, fallback: &str) -> String {
    let m = model.trim();
    if m.is_empty() { fallback.to_owned() } else { m.to_owned() }
}

/// The prompts each helper sends, built by the same functions as the real ones, with
/// placeholders in angle brackets for what scope fills in.
pub mod preview {
    use super::Settings;
    use crate::{goal, orchestration, search};

    fn member() -> orchestration::Member {
        orchestration::Member {
            id: "s1".into(),
            title: "<session name>".into(),
            goal: Some("<its goal and checklist>".into()),
            path: Default::default(),
            modified: 1_000,
            running: false,
            queued: 0,
        }
    }

    fn report() -> orchestration::Report {
        orchestration::Report { progress: 40, summary: "<the report's summary>".into(), ..Default::default() }
    }

    pub fn search(harness: &str) -> String {
        let harness = Settings::harness(harness, search::DEFAULT_HARNESS);
        let one = ("<session name>".to_owned(), Some("<its goal>".to_owned()), vec!["<its first prompts>".to_owned()]);
        format!("{}… one line for each of the 40 newest sessions in the open tab\n", search::ai_prompt(harness, "<what you typed>", &[one]))
    }

    pub fn orchestrator_question(harness: &str) -> String {
        let harness = Settings::harness(harness, orchestration::DEFAULT_HARNESS);
        let orch = orchestration::Orchestration { report: Some(report()), ..Default::default() };
        let digest = ["<a short digest of its conversation>\n".to_owned()];
        orchestration::ask_request(harness, &[member()], &digest, &["s1".into()], Some(&orch), "<your question>")
            .replace("\n## Session 1:", "\n(… every session of the scope, newest first)\n## Session 1:")
    }

    pub fn orchestrator_report(harness: &str) -> String {
        let harness = Settings::harness(harness, orchestration::DEFAULT_HARNESS);
        orchestration::request(harness, &[member()], &[Some("<what happened since the last report>\n".into())], Some(&report()), 1_300)
            .replace("\n## Session 1:", "\n(… every session of the scope, newest first)\n## Session 1:")
    }

    pub fn question(harness: &str) -> String {
        goal::question_prompt(Settings::harness(harness, goal::QUESTION_HARNESS), Some("<the session's goal and checklist>"), "<your question>")
    }

    pub fn name() -> String {
        crate::runner::name_request("<the first prompt>", "<Claude's first reply>")
    }

    pub fn goal() -> String {
        crate::runner::goal_request("<a digest of the conversation>")
    }

    pub fn tag() -> String {
        crate::verdict::request("<session name>", "<the end of the conversation>")
    }

    pub fn refine() -> String {
        crate::refine::request("<what you typed>", Some("<the session's goal>"), &["<your previous prompts>".into()], None)
    }

    pub fn overview() -> String {
        crate::overview::request("<each session: what was said of it, or its new conversation>", Some("<the scope as written before>"), Some("<git branch>"), &["<Jira keys>".into()], &[1])
    }

    pub fn every_session() -> String {
        crate::control::system_prompt("<scope>")
    }
}

/// Where updates stand, with the button for the next step (check, install, retry).
fn update_status(ui: &mut egui::Ui, u: &mut crate::update::Updater) {
    use crate::update::State;
    let ctx = ui.ctx().clone();
    let page = |ui: &mut egui::Ui, url: &str| {
        if !url.is_empty() && ui.small_button("What's new").clicked() {
            let _ = std::process::Command::new("open").arg(url).spawn();
        }
    };
    match &u.state {
        State::Idle => {
            if ui.small_button("Check for updates").clicked() {
                u.check_now(&ctx);
            }
        }
        State::Checking => {
            ui.spinner();
            ui.label(RichText::new("checking…").weak());
        }
        State::UpToDate => {
            ui.label(RichText::new("up to date").weak());
            if ui.small_button("Check again").clicked() {
                u.check_now(&ctx);
            }
        }
        State::Available(r) => {
            let (version, url) = (r.version.clone(), r.page.clone());
            ui.label(RichText::new(format!("{version} is available")).strong());
            if ui.button(format!("Install {version} and restart")).clicked() {
                u.install(&ctx);
            }
            page(ui, &url);
        }
        State::Installing(r) => {
            ui.spinner();
            ui.label(format!("installing {}…", r.version));
        }
        State::Ready(r) => {
            ui.label(format!("{} installed, restarting…", r.version));
        }
        State::Failed(e) => {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
            if ui.small_button("Try again").clicked() {
                u.check_now(&ctx);
            }
        }
    }
}
/// A harness's text: several lines, wide enough to read and edit as a paragraph. Emptying it
/// brings back the default (shown as the hint).
fn harness_field(ui: &mut egui::Ui, text: &mut String) {
    let width = ui.available_width().clamp(260.0, 760.0);
    ui.add(egui::TextEdit::multiline(text).desired_rows(4).desired_width(width).hint_text("empty = the default"));
}

/// Lays out groups, cards and rows, skipping the ones the search filters out. A group or card
/// is drawn only when something in it matches: its body runs once to count (`probe`), then
/// again to draw.
struct Page<'a, 'q> {
    ui: &'a mut egui::Ui,
    q: &'q str,
    /// The titles and blurbs of the group and card the rows are in: they match too.
    context: String,
    probe: bool,
    /// Rows (and other searchable items) shown, or that would be.
    hits: usize,
    label_width: f32,
}

impl<'q> Page<'_, 'q> {
    /// True (and counted) when an item with this text is shown; false in the counting pass.
    fn visible(&mut self, text: &str) -> bool {
        if !matches(self.q, &self.context, text) {
            return false;
        }
        self.hits += 1;
        !self.probe
    }

    /// Runs `body` to count its matches; when there are some (and this is not a counting
    /// pass), draws it inside the frame `show`, which draws its header and calls back.
    fn nest(&mut self, words: &str, label_width: f32, mut body: impl FnMut(&mut Page<'_, 'q>), show: impl FnOnce(&mut egui::Ui, &mut dyn FnMut(&mut egui::Ui))) {
        let context = format!("{} {words}", self.context);
        let mut probe = Page { ui: &mut *self.ui, q: self.q, context: context.clone(), probe: true, hits: 0, label_width };
        body(&mut probe);
        let hits = probe.hits;
        self.hits += hits;
        if hits == 0 || self.probe {
            return;
        }
        let q = self.q;
        show(self.ui, &mut |ui| {
            let mut inner = Page { ui, q, context: context.clone(), probe: false, hits: 0, label_width };
            body(&mut inner);
        });
    }

    /// A group: a card like a scope's, with its color as a stripe down the left edge.
    fn group(&mut self, g: &Group, body: impl FnMut(&mut Page<'_, 'q>)) {
        self.nest(&format!("{} {}", g.title, g.about), 300.0, body, |ui, inner| {
            ui.add_space(6.0);
            let shown = crate::overview::card_frame(ui.visuals()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(g.title.to_uppercase()).strong().size(15.0).color(g.color));
                    if !g.about.is_empty() {
                        ui.label(RichText::new(g.about).weak());
                    }
                });
                ui.add_space(4.0);
                inner(ui);
            });
            crate::overview::stripe(ui, shown.response.rect, g.color);
            ui.add_space(4.0);
        });
    }

    /// A card inside a group, for settings that belong to one place in the app: a filled
    /// bubble, like a session's, headed by its name and where it is.
    fn card(&mut self, title: &str, place: &str, body: impl FnMut(&mut Page<'_, 'q>)) {
        self.nest(&format!("{title} {place}"), 240.0, body, |ui, inner| {
            ui.add_space(6.0);
            crate::overview::tile_frame(ui.visuals(), false).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(title).strong().size(14.0));
                    ui.label(RichText::new(place).weak().small());
                });
                ui.add_space(4.0);
                inner(ui);
            });
            ui.add_space(2.0);
        });
    }

    /// Explanatory text, shown with its group (it is found by the group's words only).
    fn note(&mut self, text: &str) {
        if self.probe {
            return;
        }
        self.ui.label(RichText::new(text).weak());
        self.ui.add_space(4.0);
    }

    /// What a card's helper is, in short lines: what → value.
    fn facts(&mut self, facts: &[(&str, String)]) {
        if self.probe {
            return;
        }
        egui::Grid::new(self.ui.next_auto_id()).num_columns(2).spacing([12.0, 2.0]).show(self.ui, |ui| {
            for (what, value) in facts {
                ui.label(RichText::new(*what).weak().small());
                ui.label(RichText::new(value).small());
                ui.end_row();
            }
        });
        self.ui.add_space(6.0);
    }

    /// A harness: its label (and ↺ when edited), then its text, full width.
    fn harness(&mut self, label: &str, changed: bool, text: &mut String, default: &str) {
        if !self.visible(&format!("{label} harness instructions")) {
            return;
        }
        self.ui.horizontal(|ui| {
            ui.label(label);
            if changed {
                ui.label(RichText::new("edited").weak().small());
                if ui.small_button("↺").on_hover_text("Reset to default").clicked() {
                    *text = default.to_owned();
                }
            } else {
                ui.label(RichText::new("default").weak().small());
            }
        });
        harness_field(self.ui, text);
        self.ui.add_space(4.0);
    }

    /// The whole prompt a helper sends, folded away until opened, with a Copy button.
    fn preview(&mut self, label: &str, text: &str) {
        if !self.visible(label) {
            return;
        }
        let id = self.ui.make_persistent_id(("prompt", &self.context, label));
        egui::CollapsingHeader::new(RichText::new(label).small()).id_salt(id).show(self.ui, |ui| prompt_text(ui, text));
    }

    /// A fixed helper: what it is and when it runs, with its prompt folded under it.
    fn preview_row(&mut self, what: &str, when: &str, text: &str) {
        if !self.visible(&format!("{what} {when}")) {
            return;
        }
        let id = self.ui.make_persistent_id(("prompt", what));
        egui::CollapsingHeader::new(what).id_salt(id).show(self.ui, |ui| {
            ui.label(RichText::new(when).weak().small());
            prompt_text(ui, text);
        });
    }

    /// One setting: label, its control, a reset button when it differs from the default, help.
    /// `add` is called again with `true` when reset is clicked.
    fn row(&mut self, label: &str, help: &str, changed: bool, mut add: impl FnMut(&mut egui::Ui, bool)) {
        if !self.visible(&format!("{label} {help}")) {
            return;
        }
        let width = self.label_width;
        // Wraps when the main area is narrow (side panels wide, or a big zoom).
        self.ui.horizontal_wrapped(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(width, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(width);
                ui.add(egui::Label::new(label).truncate());
            });
            add(ui, false);
            if changed && ui.small_button("↺").on_hover_text("Reset to default").clicked() {
                add(&mut ui.new_child(egui::UiBuilder::new().invisible()), true);
            }
            if !help.is_empty() {
                ui.label(RichText::new(help).weak().small());
            }
        });
    }
}

/// A prompt as it is sent: monospace, selectable, scrolling when long, with Copy.
fn prompt_text(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(ui.visuals().extreme_bg_color).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if ui.small_button("Copy").clicked() {
            ui.ctx().copy_text(text.to_owned());
        }
        egui::ScrollArea::vertical().id_salt(("prompt-text", text.len())).max_height(280.0).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).monospace().small()).selectable(true).wrap());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_theme_only_file_loads_with_defaults() {
        let path = std::env::temp_dir().join(format!("scope-settings-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"theme":"dark"}"#).unwrap();
        let load = || crate::shared::Shared::<Settings>::load(path.clone());
        let mut s = load();
        assert_eq!(s.theme(), ThemePreference::Dark);
        assert_eq!(s.helper_model, "haiku");
        assert!(s.restore_state);
        s.zoom = 1.25;
        s.save().unwrap();
        assert_eq!(*load(), *s);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(*load(), Settings::default());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn harness_falls_back_to_its_default_when_emptied() {
        let s = Settings::default();
        assert_eq!(s.search_harness, crate::search::DEFAULT_HARNESS);
        assert_eq!(Settings::harness("  \n", "D"), "D");
        assert_eq!(Settings::harness("mine", "D"), "mine");
        assert!(matches("harness", "Harness", "Scope orchestration"));
    }

    #[test]
    fn search_matches_label_or_section_words() {
        assert!(matches("", "Files", "Tab width"));
        assert!(matches("tab", "Files", "Tab width"));
        assert!(matches("files WIDTH", "Files", "Tab width"));
        assert!(!matches("zoom", "Files", "Tab width"));
    }

    #[test]
    fn shortcuts_follow_the_send_setting() {
        let mut s = Settings::default();
        assert!(shortcuts(&s).iter().any(|(_, k)| k == "Enter"));
        assert!(shortcuts(&s).iter().any(|(w, k)| w.starts_with("Dictate") && k.ends_with("+D")));
        s.send_with_modifier = true;
        assert!(shortcuts(&s).iter().any(|(w, k)| w.starts_with("Refine") && k.ends_with("Shift+Enter")));
    }
}
