//! The Templates tab (right panel), a template open in the main area, and the template
//! composer: the app side of templates.rs.
use super::*;
use crate::templates::{self, Item, Kind};

/// How often the Templates tab reads its folders again while it shows.
const RESCAN: std::time::Duration = std::time::Duration::from_secs(2);

/// The composer's tab title: the template's name once it has one.
pub(super) fn composer_title(c: &Composer) -> String {
    match c.preview.as_deref().map(templates::front_matter).and_then(|(m, _)| m.get("name").cloned()) {
        Some(name) => format!("✨ {name}"),
        None => "✨ New template".to_owned(),
    }
}

/// The Templates tab's tree: groups open and close (closed at first, kept per scope root by
/// egui); a click on an item sets `open`.
fn tree_ui(ui: &mut egui::Ui, nodes: &[templates::Node], items: &[Item], kind: Kind, shown: Option<&std::path::Path>, open: &mut Option<Item>) {
    for node in nodes {
        match node {
            templates::Node::Group { label, key, children } => {
                let title = format!("{label}  ({})", node.count());
                egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(("templates-group", kind, key)).default_open(false).show(ui, |ui| {
                    tree_ui(ui, children, items, kind, shown, open);
                });
            }
            templates::Node::Leaf { item, label } => {
                let it = &items[*item];
                let hover = format!("{}\n{}", it.name, it.path.display());
                let resp = ui.selectable_label(shown == Some(it.path.as_path()), RichText::new(label).strong()).on_hover_text(hover);
                if !it.description.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&it.description).weak().small()).truncate());
                }
                ui.add_space(2.0);
                if resp.clicked() {
                    *open = Some(it.clone());
                }
            }
        }
    }
}

impl App {
    /// The root folder of what is open: the scope's folder; in All sessions, that of the open
    /// session's scope, else the open session's own folder.
    pub(super) fn template_root(&self) -> Option<PathBuf> {
        if let Some(d) = self.workspace_folder() {
            return Some(d);
        }
        let id = self.open_id();
        let scope = id.and_then(|id| self.workspaces.list.iter().find(|w| w.sessions.contains(&id)));
        scope.and_then(|w| w.folder.clone()).or_else(|| self.open.as_ref()?.cwd.clone()).filter(|d| d.is_dir())
    }

    fn templates_dir(&self, root: &std::path::Path) -> PathBuf {
        templates::folder_in(root, &self.settings.templates_folder)
    }

    pub(super) fn templates_panel(&mut self, ui: &mut egui::Ui) {
        let Some(root) = self.template_root() else {
            ui.label("Open a scope or a session to see the prompt templates and skills of its folder.");
            return;
        };
        let stale = self.template_items.as_ref().is_none_or(|(r, at, _)| *r != root || at.elapsed() > RESCAN);
        if stale {
            let home = crate::platform::home();
            let items = templates::scan(&root, &self.settings.templates_folder, &home);
            self.template_items = Some((root.clone(), std::time::Instant::now(), items));
            ui.ctx().request_repaint_after(RESCAN);
        }
        let dir = self.templates_dir(&root);
        ui.label(RichText::new(root.display().to_string()).weak().small()).on_hover_text("The scope's root folder");
        ui.add_space(4.0);
        let create = egui::Button::new("✨  Create template").min_size(egui::vec2(ui.available_width(), 26.0));
        let mut open = None;
        let mut compose = false;
        if ui.add(create).on_hover_text(format!("Claude writes it, in {}", dir.display())).clicked() {
            compose = true;
        }
        let items = self.template_items.as_ref().map(|t| t.2.clone()).unwrap_or_default();
        let shown = self.template_view.as_ref().filter(|_| self.page == Some(Page::Template)).map(|v| v.item.path.clone());
        egui::ScrollArea::vertical().id_salt("templates-list").auto_shrink(false).show(ui, |ui| {
            let mut kinds: Vec<Kind> = items.iter().map(|i| i.kind).collect();
            kinds.dedup();
            if !kinds.contains(&Kind::Template) {
                kinds.insert(0, Kind::Template);
            }
            for kind in kinds {
                let of: Vec<usize> = (0..items.len()).filter(|&i| items[i].kind == kind).collect();
                super::section(ui, &format!("{} · {}", kind.heading(), of.len()));
                if of.is_empty() {
                    ui.label(RichText::new("None yet: Create template has Claude write one.").weak().small());
                }
                tree_ui(ui, &templates::tree(&items, &of), &items, kind, shown.as_deref(), &mut open);
            }
        });
        if let Some(item) = open {
            self.open_template(item, root.clone());
        }
        if compose {
            self.open_composer(root, None);
        }
    }

    /// Shows `item` in the main area, in place of the one shown before.
    pub(super) fn open_template(&mut self, item: Item, root: PathBuf) {
        let contents = std::fs::read_to_string(&item.path).map_err(|e| e.to_string());
        let rendered = self.settings.markdown_rendered;
        self.template_view = Some(TemplateView { item, root, contents, rendered });
        self.page = Some(Page::Template);
        self.settings_active = false;
    }

    /// The template open in the main area: its name, where it is, then its text (rendered or
    /// as source), with Copy, Use in prompt and, for a template, Change with Claude.
    pub(super) fn template_page(&mut self, ui: &mut egui::Ui) {
        let opts = self.file_options();
        let can_use = self.open.is_some();
        let Some(v) = &mut self.template_view else { return };
        let text = v.contents.as_deref().unwrap_or_default().to_owned();
        let (mut copy, mut use_it, mut change) = (false, false, false);
        ui.heading(&v.item.name);
        ui.label(RichText::new(format!("{} · {}", v.item.kind.heading(), v.item.path.display())).weak().small());
        if !v.item.description.is_empty() {
            ui.label(&v.item.description);
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut v.rendered, true, "Rendered");
            ui.selectable_value(&mut v.rendered, false, "Source");
            ui.separator();
            copy = ui.button("Copy").on_hover_text("The whole file").clicked();
            let why = (!can_use).then_some("Open a session first");
            let label = if v.item.kind.is_template() { "Use in prompt" } else { "Use in prompt (/name)" };
            use_it = ui.add_enabled(can_use, egui::Button::new(label)).on_hover_text("Puts it in the open session's input, to edit and send").on_disabled_hover_text(why.unwrap_or_default()).clicked();
            if v.item.kind.is_template() {
                change = ui.button("✎ Change with Claude").on_hover_text("Opens the composer on this template").clicked();
            }
        });
        ui.separator();
        match (&v.contents, v.rendered) {
            (Ok(_), true) => {
                let body = templates::front_matter(&text).1.to_owned();
                egui::ScrollArea::vertical().id_salt(("template", v.item.path.clone())).auto_shrink(false).show(ui, |ui| {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| markdown::show(ui, &mut self.md_cache, &body));
                });
            }
            (contents, _) => diffview::file_view(ui, &v.item.path.display().to_string(), contents, &opts, ("template-source", v.item.path.clone()), None),
        }
        if copy {
            ui.ctx().copy_text(text.clone());
        }
        if use_it {
            let insert = templates::use_text(&v.item, &text);
            self.input = insert;
            self.page = None;
            self.focus_prompt = true;
        }
        if change {
            let (root, path) = (v.root.clone(), v.item.path.clone());
            self.open_composer(root, Some(path));
        }
    }

    /// Opens the composer in `root`: on `template`, through the session that wrote it when
    /// scope has it (so Claude has that conversation), else a new one.
    pub(super) fn open_composer(&mut self, root: PathBuf, template: Option<PathBuf>) {
        if let Some(c) = &self.composer
            && c.template.is_some()
            && c.template == template
        {
            self.page = Some(Page::Composer);
            self.settings_active = false;
            return;
        }
        if let Some(run) = self.composer.take().and_then(|c| c.run) {
            run.stop.stop();
        }
        let dir = self.templates_dir(&root);
        let known = template.as_deref().and_then(|t| self.template_store.session_of(t)).map(str::to_owned);
        let file = known.as_deref().and_then(|id| templates::session_file(&session::projects_dir(), id));
        let (id, started, mut session) = match (known, file) {
            (Some(id), Some(f)) => (id, true, session::load(&f).unwrap_or_default()),
            _ => (runner::new_session_id(), false, Session::default()),
        };
        // Its prompts show as typed, without the sessions scope added to them.
        for m in session.messages.iter_mut().filter(|m| m.role == Role::User) {
            m.text = templates::typed(&m.text).to_owned();
        }
        let preview = template.as_ref().and_then(|t| std::fs::read_to_string(t).ok());
        self.composer = Some(Composer { id, root, dir, started, session, run: None, input: String::new(), template, preview, error: None, focus: true });
        self.page = Some(Page::Composer);
        self.settings_active = false;
    }

    /// What a composer prompt's `@[name]` brings along: where the session's transcript is,
    /// its folder, and a digest of its conversation.
    fn reference(&self, name: &str) -> Result<templates::Reference, String> {
        let id = self.find_session(name)?;
        let i = self.sessions.iter().position(|m| m.id == id).ok_or("it has no file yet")?;
        let meta = &self.sessions[i];
        let s = session::load(&meta.path).map_err(|e| format!("can't read it: {e}"))?;
        Ok(templates::Reference {
            name: self.title(i).to_owned(),
            transcript: meta.path.clone(),
            folder: s.cwd.clone(),
            digest: session::digest(&s.messages, 600, 16_000),
        })
    }

    /// Enter in the composer: sends the prompt (with what its `@[name]`s refer to) to the
    /// composer's session, set up so Claude can only write templates.
    fn send_composer(&mut self) {
        let Some(c) = &self.composer else { return };
        let typed = c.input.trim().to_owned();
        if c.run.is_some() || typed.is_empty() {
            return;
        }
        let expanded = templates::expand(&typed, |name| self.reference(name));
        let (id, root) = (c.id.clone(), c.root.clone());
        let prompt = match expanded {
            Ok(p) => p,
            Err(e) => {
                self.composer.as_mut().unwrap().error = Some(e);
                return;
            }
        };
        // Listed as the composer's before its file exists, so the session list never shows it.
        let entry_root = root.clone();
        if let Err(e) = self.template_store.change(|st| st.sessions.entry(id.clone()).or_default().root = entry_root) {
            self.composer.as_mut().unwrap().error = Some(format!("could not save {}: {e}", templates::Store::path().display()));
            return;
        }
        let gitignore = self.settings.templates_gitignore;
        let c = self.composer.as_mut().unwrap();
        let args = templates::run_args(&c.root, &c.dir, gitignore, c.template.as_deref());
        let (rx, stop) = runner::compose(self.ctx.clone(), id, root, prompt, !c.started, args);
        c.session.push_prompt(typed);
        c.input.clear();
        c.error = None;
        c.run = Some(ComposerRun { rx, stop, live: String::new(), tool: None, since: SystemTime::now() - std::time::Duration::from_secs(1), stopped: false });
    }

    /// The composer's run: its steps join the conversation as they come; the template it
    /// writes shows (and is remembered as this session's) as soon as it is there.
    pub(super) fn poll_composer(&mut self) {
        let Some(c) = &mut self.composer else { return };
        let Some(run) = &mut c.run else { return };
        let mut done = None;
        let mut reread = false;
        for event in run.rx.try_iter() {
            match event {
                RunEvent::Line(v) => {
                    c.session.apply(&v);
                    run.live.clear();
                    run.tool = None;
                    if let Some(p) = templates::written(&v, &c.dir) {
                        c.template = Some(p);
                    }
                    reread = true;
                }
                RunEvent::Delta(t) => run.live.push_str(&t),
                RunEvent::Tool(name) => run.tool = Some(name),
                RunEvent::Done(result) => done = Some(result),
            }
        }
        let Some(result) = done else {
            if reread && let Some(t) = &c.template {
                c.preview = std::fs::read_to_string(t).ok().or(c.preview.take());
            }
            return;
        };
        let run = c.run.take().unwrap();
        match result {
            Err(_) if run.stopped => c.session.push_prompt("[Request interrupted by user]".to_owned()),
            Err(e) => c.error = Some(e),
            Ok(()) => {}
        }
        c.started |= templates::session_file(&session::projects_dir(), &c.id).is_some();
        if c.template.is_none() {
            c.template = templates::changed_since(&c.dir, run.since);
        }
        let Some(t) = c.template.clone() else { return };
        c.preview = std::fs::read_to_string(&t).ok();
        let id = c.id.clone();
        let template = t.clone();
        if let Err(e) = self.template_store.change(|st| st.sessions.entry(id).or_default().template = Some(template)) {
            self.error = Some(format!("could not save {}: {e}", templates::Store::path().display()));
        }
        // The list and an open copy of it show what was written.
        self.template_items = None;
        if let Some(v) = self.template_view.as_mut().filter(|v| v.item.path == t) {
            v.contents = std::fs::read_to_string(&t).map_err(|e| e.to_string());
        }
    }

    /// The composer: the template once there is one, the conversation with Claude, and the
    /// field to ask for it (`@[name]` refers to a session; @ Session picks one).
    pub(super) fn composer_page(&mut self, ui: &mut egui::Ui) {
        let mut names: Vec<(SystemTime, String)> = self.visible().into_iter().map(|i| (self.sessions[i].modified, self.title(i).to_owned())).collect();
        names.sort_by_key(|n| std::cmp::Reverse(n.0));
        names.truncate(40);
        let can_use = self.open.is_some();
        let pal = theme::palette(ui.visuals());
        let Some(c) = &mut self.composer else { return };
        let (mut send, mut stop, mut open, mut use_it) = (false, false, false, false);
        let field_id = Id::new("template-input");

        egui::Panel::bottom("composer-input").frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6))).show_separator_line(false).resizable(false).show(ui, |ui| {
            if let Some(e) = &c.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.horizontal(|ui| {
                ui.menu_button("@ Session", |ui| {
                    ui.label(RichText::new("Refer to a session: Claude reads what it did").weak().small());
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for (_, name) in &names {
                            if ui.button(name).clicked() {
                                if !c.input.is_empty() && !c.input.ends_with([' ', '\n']) {
                                    c.input.push(' ');
                                }
                                c.input.push_str(&format!("@[{name}] "));
                                c.focus = true;
                                ui.close();
                            }
                        }
                    });
                });
                ui.label(RichText::new("Claude only writes Markdown files in the templates folder").weak().small()).on_hover_text(c.dir.display().to_string());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if c.run.is_some() {
                        stop = ui.button("⏹ Stop").clicked();
                        ui.spinner();
                    } else {
                        send = ui.add_enabled(!c.input.trim().is_empty(), egui::Button::new("Send")).clicked();
                    }
                });
            });
            if ui.memory(|m| m.has_focus(field_id)) && c.run.is_none() && !c.input.trim().is_empty() {
                send |= ui.input_mut(|i| {
                    let enter = i.key_pressed(Key::Enter) && !i.modifiers.shift;
                    if enter {
                        i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::Enter, .. }));
                    }
                    enter
                });
            }
            let hint = "Describe the template, e.g.: a prompt that does what @[Fix login redirect] did, for any page";
            ui.add(egui::TextEdit::multiline(&mut c.input).id(field_id).hint_text(hint).desired_rows(3).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            if std::mem::take(&mut c.focus) {
                let mut st = egui::TextEdit::load_state(ui.ctx(), field_id).unwrap_or_default();
                st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(c.input.chars().count()))));
                st.store(ui.ctx(), field_id);
                ui.memory_mut(|m| m.request_focus(field_id));
            }
        });

        ui.heading(if c.template.is_some() { "Prompt template" } else { "New prompt template" });
        ui.label(RichText::new(format!("Claude writes it in {}; scope only reads it. Refer to a session with @[its name].", c.dir.display())).weak().small());
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt(("composer", c.id.clone())).auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
            if let (Some(path), Some(text)) = (&c.template, &c.preview) {
                let (meta, body) = templates::front_matter(text);
                egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("📝 {}", meta.get("name").map_or("Template", |n| n.as_str()))).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            use_it = ui.add_enabled(can_use, egui::Button::new("Use in prompt")).on_disabled_hover_text("Open a session first").clicked();
                            open = ui.button("Open").on_hover_text("In its own tab").clicked();
                        });
                    });
                    ui.label(RichText::new(path.display().to_string()).weak().small());
                    if let Some(d) = meta.get("description") {
                        ui.label(d);
                    }
                    ui.separator();
                    markdown::show(ui, &mut self.md_cache, body);
                });
                ui.add_space(8.0);
            }
            if c.session.messages.is_empty() && c.run.is_none() {
                ui.label(RichText::new("Say what the template should do. Claude asks if it needs to know more.").weak());
            }
            for (i, m) in c.session.messages.iter().enumerate() {
                let shown;
                let m = if m.role == Role::User && !templates::references(&m.text).is_empty() {
                    shown = session::Message { role: Role::User, text: templates::display(&m.text) };
                    &shown
                } else {
                    m
                };
                ui.push_id(("composer-message", i), |ui| super::message(ui, &mut self.md_cache, m, &pal, None));
            }
            if let Some(run) = &c.run {
                if !run.live.is_empty() {
                    let live = session::Message { role: Role::Assistant, text: run.live.clone() };
                    super::message(ui, &mut self.md_cache, &live, &pal, None);
                }
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(run.tool.as_ref().map_or("Claude is working…".to_owned(), |t| format!("⚙ {t}…"))).weak());
                });
            }
        });

        if stop && let Some(run) = &mut c.run {
            run.stopped = true;
            run.stop.stop();
        }
        let shown = c.template.clone().zip(c.preview.clone()).map(|(p, text)| (p, text, c.root.clone()));
        if send {
            self.send_composer();
            if let Some(c) = &mut self.composer {
                c.focus = true;
            }
        }
        let Some((path, text, root)) = shown else { return };
        if use_it {
            let (meta, _) = templates::front_matter(&text);
            let item = Item { kind: Kind::Template, name: meta.get("name").cloned().unwrap_or_default(), description: String::new(), folders: Vec::new(), path };
            self.input = templates::use_text(&item, &text);
            self.page = None;
            self.focus_prompt = true;
        } else if open {
            let home = crate::platform::home();
            let found = templates::scan(&root, &self.settings.templates_folder, &home).into_iter().find(|i| i.path == path);
            let item = found.unwrap_or(Item { kind: Kind::Template, name: "Template".into(), description: String::new(), folders: Vec::new(), path });
            self.open_template(item, root);
        }
    }
}
