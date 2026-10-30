//! Changes grouped by file, rendered as diff cards; and a read-only file viewer.
use crate::find::{self, Find};
use crate::session::Change;
use crate::theme::{self, Palette};
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, FontId, RichText, Stroke, TextWrapMode};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};
use similar::{ChangeTag, TextDiff};
use std::path::Path;

const CONTEXT_LINES: usize = 3;
const MAX_LINES_PER_EDIT: usize = 300;
const MAX_VIEW_BYTES: usize = 2_000_000;

pub struct FileChanges {
    pub file: String,
    /// Indices into the session's changes, in order.
    pub edits: Vec<usize>,
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, PartialEq)]
pub enum Line {
    Context(String),
    Added(String),
    Removed(String),
    /// Unchanged lines left out between hunks.
    Gap,
}

/// Changes grouped by file, files in order of their first change.
pub fn group(changes: &[Change]) -> Vec<FileChanges> {
    let mut files: Vec<FileChanges> = Vec::new();
    for (i, c) in changes.iter().enumerate() {
        let (added, removed) = counts(c);
        match files.iter_mut().find(|f| f.file == c.file) {
            Some(f) => {
                f.edits.push(i);
                f.added += added;
                f.removed += removed;
            }
            None => files.push(FileChanges { file: c.file.clone(), edits: vec![i], added, removed }),
        }
    }
    files
}

fn counts(c: &Change) -> (usize, usize) {
    let diff = TextDiff::from_lines(&c.old, &c.new);
    diff.iter_all_changes().fold((0, 0), |(a, r), ch| match ch.tag() {
        ChangeTag::Insert => (a + 1, r),
        ChangeTag::Delete => (a, r + 1),
        ChangeTag::Equal => (a, r),
    })
}

/// The edit as hunks with a few lines of context, unchanged stretches collapsed to `Gap`.
pub fn lines(c: &Change) -> Vec<Line> {
    let diff = TextDiff::from_lines(&c.old, &c.new);
    let mut out = Vec::new();
    for (i, group) in diff.grouped_ops(CONTEXT_LINES).iter().enumerate() {
        if i > 0 {
            out.push(Line::Gap);
        }
        for op in group {
            for ch in diff.iter_changes(op) {
                let text = ch.value().trim_end_matches(['\n', '\r']).to_owned();
                out.push(match ch.tag() {
                    ChangeTag::Insert => Line::Added(text),
                    ChangeTag::Delete => Line::Removed(text),
                    ChangeTag::Equal => Line::Context(text),
                });
            }
        }
    }
    out
}

/// (file name, parent folder) for display.
pub fn split_path(file: &str) -> (&str, &str) {
    match file.rfind('/') {
        Some(i) => (&file[i + 1..], &file[..i]),
        None => (file, ""),
    }
}

/// The file's folder as the file list shows it: relative to the session's folder ("./" for
/// the folder itself), or in full when the file is outside it.
pub fn folder_in(file: &str, cwd: Option<&Path>) -> String {
    let (_, dir) = split_path(file);
    match cwd.and_then(|c| Path::new(dir).strip_prefix(c).ok()) {
        Some(rel) if rel.as_os_str().is_empty() => "./".to_owned(),
        Some(rel) => rel.display().to_string(),
        None => dir.to_owned(),
    }
}

/// (edits, lines added, lines removed) for one file across the session's changes.
pub fn stats(changes: &[Change], file: &str) -> (usize, usize, usize) {
    changes.iter().filter(|c| c.file == file).fold((0, 0, 0), |(n, a, r), c| {
        let (added, removed) = counts(c);
        (n + 1, a + added, r + removed)
    })
}

/// A readable name for the file's language, from its extension.
pub fn language_name(file: &str) -> String {
    let name = match language(file) {
        "rs" => "Rust",
        "py" => "Python",
        "js" | "mjs" | "cjs" => "JavaScript",
        "ts" | "mts" | "cts" => "TypeScript",
        "tsx" => "TypeScript (JSX)",
        "jsx" => "JavaScript (JSX)",
        "go" => "Go",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "java" => "Java",
        "kt" => "Kotlin",
        "swift" => "Swift",
        "rb" => "Ruby",
        "sh" | "bash" | "zsh" => "Shell",
        "md" | "markdown" => "Markdown",
        "json" => "JSON",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "html" | "htm" => "HTML",
        "css" => "CSS",
        "sql" => "SQL",
        "txt" => "Text",
        other => return other.to_uppercase(),
    };
    name.to_owned()
}

fn language(file: &str) -> &str {
    Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("txt")
}

pub enum CardEvent {
    None,
    Clicked,
    DoubleClicked,
}

/// One file's changes as a card: header with name, folder and counts, then every edit's hunks.
pub fn file_card(ui: &mut egui::Ui, f: &FileChanges, changes: &[Change]) -> CardEvent {
    let mut event = CardEvent::None;
    let pal = theme::palette(ui.visuals());
    egui::Frame::new()
        .stroke(Stroke::new(1.0, pal.border))
        .corner_radius(6.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (name, dir) = split_path(&f.file);
            let header = ui
                .horizontal(|ui| {
                    ui.label(RichText::new(name).strong().monospace());
                    ui.label(RichText::new(format!("+{}", f.added)).color(pal.add_fg).monospace());
                    ui.label(RichText::new(format!("-{}", f.removed)).color(pal.del_fg).monospace());
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_text("Click to show only this file, double-click to open it");
            if header.double_clicked() {
                event = CardEvent::DoubleClicked;
            } else if header.clicked() {
                event = CardEvent::Clicked;
            }
            if !dir.is_empty() {
                ui.label(RichText::new(dir).weak().small());
            }
            let lang = language(&f.file);
            for (n, &i) in f.edits.iter().enumerate() {
                ui.add_space(4.0);
                if n > 0 {
                    ui.separator();
                }
                ui.spacing_mut().item_spacing.y = 0.0;
                let all = lines(&changes[i]);
                for line in all.iter().take(MAX_LINES_PER_EDIT) {
                    diff_line(ui, line, lang, &pal);
                }
                if all.len() > MAX_LINES_PER_EDIT {
                    ui.label(RichText::new(format!("… {} more lines", all.len() - MAX_LINES_PER_EDIT)).weak());
                }
            }
        });
    event
}

fn diff_line(ui: &mut egui::Ui, line: &Line, lang: &str, pal: &Palette) {
    let (sign, text, bg, sign_color, dim) = match line {
        Line::Added(t) => ("+", t.as_str(), Some(pal.add_bg), pal.add_fg, false),
        Line::Removed(t) => ("-", t.as_str(), Some(pal.del_bg), pal.del_fg, false),
        Line::Context(t) => (" ", t.as_str(), None, pal.context, true),
        Line::Gap => {
            ui.label(RichText::new("  ⋮").weak().monospace());
            return;
        }
    };
    let font = FontId::monospace(12.0);
    let mut job = LayoutJob::default();
    job.append(&format!("{sign} "), 0.0, TextFormat::simple(font.clone(), sign_color));
    let theme = CodeTheme::from_memory(ui.ctx(), ui.style()).with_font_id(font);
    let code = highlight(ui.ctx(), ui.style(), &theme, text, lang);
    let offset = job.text.len();
    job.text.push_str(&code.text);
    for mut section in code.sections {
        section.byte_range = section.byte_range.start + offset..section.byte_range.end + offset;
        if dim {
            section.format.color = section.format.color.gamma_multiply(0.55);
        }
        job.sections.push(section);
    }
    // Background spans the full card width, painted behind the text.
    let slot = ui.painter().add(egui::Shape::Noop);
    let resp = ui.add(egui::Label::new(job).wrap_mode(TextWrapMode::Extend));
    if let Some(bg) = bg {
        let rect = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), resp.rect.y_range());
        ui.painter().set(slot, egui::Shape::rect_filled(rect, 0.0, bg));
    }
}

/// Read-only file contents with line numbers and syntax highlighting.
/// How files are shown, from the Settings page.
pub struct FileOptions {
    pub line_numbers: bool,
    pub wrap: bool,
    pub tab_width: usize,
    pub font_size: f32,
}

/// Tabs as spaces, so they line up with the tab width setting.
pub fn expand_tabs(text: &str, width: usize) -> std::borrow::Cow<'_, str> {
    if !text.contains('\t') {
        return text.into();
    }
    let mut out = String::with_capacity(text.len());
    let mut col = 0;
    for c in text.chars() {
        match c {
            '\t' => {
                let n = width.max(1) - col % width.max(1);
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            }
            '\n' => {
                out.push(c);
                col = 0;
            }
            _ => {
                out.push(c);
                col += 1;
            }
        }
    }
    out.into()
}

/// A read-only file with line numbers and syntax highlighting. `id` keeps its scroll position.
/// With `find`, its matches (byte ranges in the tab-expanded text) are highlighted, and the
/// view scrolls to the current one when `find.scroll` is set.
pub fn file_view(ui: &mut egui::Ui, path: &str, contents: &Result<String, String>, opts: &FileOptions, id: impl std::hash::Hash + std::fmt::Debug, find: Option<&mut Find>) {
    let pal = theme::palette(ui.visuals());
    let text = match contents {
        Ok(t) => t,
        Err(e) => {
            ui.colored_label(pal.error, e);
            return;
        }
    };
    let text = expand_tabs(text, opts.tab_width);
    let font = FontId::monospace(opts.font_size);
    let theme = CodeTheme::from_memory(ui.ctx(), ui.style()).with_font_id(font.clone());
    let mut job = highlight(ui.ctx(), ui.style(), &theme, &text, language(path));
    let (ranges, current) = find.as_ref().map(|f| f.in_item(0)).unwrap_or_default();
    find::mark(&mut job, &ranges, current, pal.find);
    let count = text.lines().count().max(1);
    let numbers: String = (1..=count).map(|n| format!("{n:>5}\n")).collect();
    let scroll = if opts.wrap { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() };
    scroll.id_salt(id).auto_shrink(false).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            // Line numbers only line up with unwrapped lines.
            if opts.line_numbers && !opts.wrap {
                ui.label(RichText::new(numbers).font(font).color(pal.line_number));
            }
            // Laid out here (not by the label) so the current match can be located in it.
            job.wrap.max_width = if opts.wrap { ui.available_width() } else { f32::INFINITY };
            let galley = ui.painter().layout_job(job);
            let target = current.map(|k| {
                let start = text[..ranges[k].start].chars().count();
                galley.pos_from_cursor(egui::text::CCursor::new(start))
            });
            let mode = if opts.wrap { TextWrapMode::Wrap } else { TextWrapMode::Extend };
            let label = ui.add(egui::Label::new(galley).wrap_mode(mode).selectable(true));
            if let (Some(f), Some(r)) = (find, target)
                && f.scroll
            {
                f.scroll = false;
                ui.scroll_to_rect(r.translate(label.rect.min.to_vec2()).expand(8.0), Some(egui::Align::Center));
            }
        });
    });
}

/// File contents for the viewer, refusing very large or binary files.
pub fn read(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("can't open {path}: {e}"))?;
    if bytes.len() > MAX_VIEW_BYTES {
        return Err(format!("{path} is too large to show ({} MB)", bytes.len() / 1_000_000));
    }
    String::from_utf8(bytes).map_err(|_| format!("{path} is not a text file"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(file: &str, old: &str, new: &str) -> Change {
        Change { file: file.into(), old: old.into(), new: new.into() }
    }

    #[test]
    fn tabs_expand_to_the_next_stop() {
        assert_eq!(expand_tabs("a\tb\n\tc", 4), "a   b\n    c");
        assert_eq!(expand_tabs("ab\tc", 2), "ab  c");
        assert!(matches!(expand_tabs("no tabs", 4), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn stats_count_one_files_edits() {
        let c = vec![change("/a/x.rs", "1\n", "2\n"), change("/a/y.rs", "", "new\n"), change("/a/x.rs", "3\n", "4\n5\n")];
        assert_eq!(stats(&c, "/a/x.rs"), (2, 3, 2));
        assert_eq!(stats(&c, "/a/z.rs"), (0, 0, 0));
        assert_eq!(language_name("/a/x.rs"), "Rust");
        assert_eq!(language_name("/a/Makefile"), "Text");
        assert_eq!(language_name("/a/x.zig"), "ZIG");
    }

    #[test]
    fn groups_by_file_in_first_seen_order() {
        let c = vec![change("/a/x.rs", "1\n", "2\n"), change("/a/y.rs", "", "new\n"), change("/a/x.rs", "3\n", "4\n5\n")];
        let g = group(&c);
        assert_eq!(g.iter().map(|f| f.file.as_str()).collect::<Vec<_>>(), vec!["/a/x.rs", "/a/y.rs"]);
        assert_eq!(g[0].edits, vec![0, 2]);
        assert_eq!((g[0].added, g[0].removed), (3, 2));
        assert_eq!(split_path("/a/x.rs"), ("x.rs", "/a"));
    }

    #[test]
    fn folders_are_relative_to_the_session_folder() {
        let cwd = Some(Path::new("/a/proj"));
        assert_eq!(folder_in("/a/proj/x.rs", cwd), "./");
        assert_eq!(folder_in("/a/proj/src/e2e/x.rs", cwd), "src/e2e");
        assert_eq!(folder_in("/a/project/x.rs", cwd), "/a/project");
        assert_eq!(folder_in("/b/x.rs", None), "/b");
    }

    #[test]
    fn collapses_unchanged_lines_between_hunks() {
        let old: String = (1..=20).map(|n| format!("line{n}\n")).collect();
        let new = old.replace("line2\n", "two\n").replace("line19\n", "nineteen\n");
        let l = lines(&change("f", &old, &new));
        assert!(l.contains(&Line::Gap));
        assert!(l.contains(&Line::Removed("line2".into())) && l.contains(&Line::Added("two".into())));
        assert!(l.contains(&Line::Context("line5".into())) && !l.contains(&Line::Context("line10".into())));
    }
}
