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
    ui.separator();
    let q = filter.trim().to_owned();
    egui::ScrollArea::vertical().id_salt("settings").auto_shrink(false).show(ui, |ui| {
        let mut page = Page { ui, q: &q, section: "", headed: "", shown: false };

        page.section("Appearance");
        page.row("Theme", "Dark, light, or follow the system", s.theme != defaults.theme, |ui, reset| {
            if reset {
                s.theme = defaults.theme.clone();
            }
            for (v, label) in [("dark", "🌙 Dark"), ("light", "☀ Light"), ("system", "💻 System")] {
                ui.selectable_value(&mut s.theme, v.to_owned(), label);
            }
        });
        page.row("Monospace font size", "Prompt input, code and files", s.mono_size != defaults.mono_size, |ui, reset| {
            if reset {
                s.mono_size = defaults.mono_size;
            }
            ui.add(egui::Slider::new(&mut s.mono_size, 9.0..=20.0).step_by(0.5));
        });

        page.section("Prompt input");
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

        page.section("Conversation");
        page.row("Show tool lines", "The dimmed ⚙ Read · Edit lines between messages", s.show_tool_lines != defaults.show_tool_lines, |ui, reset| {
            if reset {
                s.show_tool_lines = defaults.show_tool_lines;
            }
            ui.checkbox(&mut s.show_tool_lines, "");
        });

        page.section("Files");
        page.row("Line numbers", "In open files", s.line_numbers != defaults.line_numbers, |ui, reset| {
            if reset {
                s.line_numbers = defaults.line_numbers;
            }
            ui.checkbox(&mut s.line_numbers, "");
        });
        page.row("Wrap long lines", "Off: scroll sideways", s.wrap_file_lines != defaults.wrap_file_lines, |ui, reset| {
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

        page.section("Claude");
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
        page.row("Helper model", "Search, goals and Refine", s.helper_model != defaults.helper_model, |ui, reset| {
            if reset {
                s.helper_model = defaults.helper_model.clone();
            }
            ui.add(egui::TextEdit::singleline(&mut s.helper_model).desired_width(260.0));
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

        page.section("Behaviour");
        page.row("Confirm before deleting a scope", "", s.confirm_delete_workspace != defaults.confirm_delete_workspace, |ui, reset| {
            if reset {
                s.confirm_delete_workspace = defaults.confirm_delete_workspace;
            }
            ui.checkbox(&mut s.confirm_delete_workspace, "");
        });
        page.row("Restore the previous state at start", "Open session, tabs, drafts and queues", s.restore_state != defaults.restore_state, |ui, reset| {
            if reset {
                s.restore_state = defaults.restore_state;
            }
            ui.checkbox(&mut s.restore_state, "");
        });
        page.row("Notify when a run finishes", "A system notification, while scope is in the background", s.notify_when_done != defaults.notify_when_done, |ui, reset| {
            if reset {
                s.notify_when_done = defaults.notify_when_done;
            }
            ui.checkbox(&mut s.notify_when_done, "");
        });
        page.row("Tag finished sessions", "After a run ends, the helper model reads the last messages and tags the session done, needs you or blocked", s.tag_finished != defaults.tag_finished, |ui, reset| {
            if reset {
                s.tag_finished = defaults.tag_finished;
            }
            ui.checkbox(&mut s.tag_finished, "");
        });

        page.section("Overview");
        page.row("Jira URL", "Ticket keys in the scopes overview link here, e.g. https://acme.atlassian.net", s.jira_url != defaults.jira_url, |ui, reset| {
            if reset {
                s.jira_url = defaults.jira_url.clone();
            }
            ui.add(egui::TextEdit::singleline(&mut s.jira_url).hint_text("https://…atlassian.net").desired_width(260.0));
        });
        page.row("Offer to finish sessions idle for (hours)", "They show Finish session, to take them out of the scope; 0 = never", s.stale_after_hours != defaults.stale_after_hours, |ui, reset| {
            if reset {
                s.stale_after_hours = defaults.stale_after_hours;
            }
            ui.add(egui::Slider::new(&mut s.stale_after_hours, 0..=168));
        });

        page.section("Harness");
        page.row("Scope orchestration", "What the orchestrator of each scope is told to do; the reply format and the sessions are added after it", s.orchestration_harness != defaults.orchestration_harness, |ui, reset| {
            if reset {
                s.orchestration_harness = defaults.orchestration_harness.clone();
            }
            harness_field(ui, &mut s.orchestration_harness);
        });
        page.row("Orchestration model", "e.g. sonnet, opus; empty = the helper model", s.orchestration_model != defaults.orchestration_model, |ui, reset| {
            if reset {
                s.orchestration_model = defaults.orchestration_model.clone();
            }
            ui.add(egui::TextEdit::singleline(&mut s.orchestration_model).hint_text("helper model").desired_width(260.0));
        });
        page.row("Update orchestration every (minutes)", "While its sessions have new activity and its page is open; 0 = only when asked", s.orchestration_every != defaults.orchestration_every, |ui, reset| {
            if reset {
                s.orchestration_every = defaults.orchestration_every;
            }
            ui.add(egui::Slider::new(&mut s.orchestration_every, 0..=120));
        });
        page.row("Questions (right panel)", "How a question in Info → Ask about this goal starts; the goal, its checklist and the question follow", s.question_harness != defaults.question_harness, |ui, reset| {
            if reset {
                s.question_harness = defaults.question_harness.clone();
            }
            harness_field(ui, &mut s.question_harness);
        });
        page.row("Finding sessions (left panel)", "What the helper model is told when Enter in the search asks it; the query, reply format and session list follow", s.search_harness != defaults.search_harness, |ui, reset| {
            if reset {
                s.search_harness = defaults.search_harness.clone();
            }
            harness_field(ui, &mut s.search_harness);
        });

        page.section("Keyboard shortcuts");
        for (what, keys) in shortcuts(s) {
            page.row(what, "", false, |ui, _| {
                ui.label(RichText::new(&keys).monospace());
            });
        }

        page.section("About");
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
        if !page.shown {
            page.ui.label("No setting matches.");
        }
    });
    *s != before
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
    let width = ui.available_width().clamp(260.0, 640.0);
    ui.add(egui::TextEdit::multiline(text).desired_rows(4).desired_width(width).hint_text("empty = the default"));
}

/// Lays out sections and rows, skipping the ones the search filters out.
struct Page<'a, 'q> {
    ui: &'a mut egui::Ui,
    q: &'q str,
    section: &'static str,
    /// Section whose heading was drawn last.
    headed: &'static str,
    shown: bool,
}

impl Page<'_, '_> {
    fn section(&mut self, name: &'static str) {
        self.section = name;
    }

    /// One setting: label, its control, a reset button when it differs from the default, help.
    /// `add` is called again with `true` when reset is clicked.
    fn row(&mut self, label: &str, help: &str, changed: bool, mut add: impl FnMut(&mut egui::Ui, bool)) {
        if !matches(self.q, self.section, &format!("{label} {help}")) {
            return;
        }
        if self.headed != self.section {
            self.headed = self.section;
            self.ui.add_space(12.0);
            self.ui.label(RichText::new(self.section).strong().size(15.0));
            self.ui.add_space(2.0);
        }
        self.shown = true;
        // Wraps when the main area is narrow (side panels wide, or a big zoom).
        self.ui.horizontal_wrapped(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(300.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(300.0);
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
