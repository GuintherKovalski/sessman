use eframe::egui::{self, Align, FontId, Sense, Stroke, TextFormat, TextStyle};
use eframe::egui::text::LayoutJob;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

/// The "Show tool lines" setting.
pub static SHOW_TOOLS: AtomicBool = AtomicBool::new(true);

/// A message split into what sessman draws itself (tables, tool calls) and plain Markdown.
#[derive(Debug, PartialEq)]
pub enum Block<'a> {
    Markdown(&'a str),
    Table(Table),
    /// Names of consecutive tool calls (`[tool: Name]` lines).
    Tools(Vec<&'a str>),
}

/// Cells hold their Markdown source; every row has as many cells as the header.
#[derive(Debug, PartialEq)]
pub struct Table {
    pub align: Vec<Alignment>,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

pub fn blocks(text: &str) -> Vec<Block<'_>> {
    let mut out = Vec::new();
    let mut start = 0; // start of the pending Markdown chunk
    let mut pos = 0;
    for line in text.split_inclusive('\n') {
        if let Some(name) = line.trim_end().strip_prefix("[tool: ").and_then(|l| l.strip_suffix(']')) {
            markdown_blocks(&text[start..pos], &mut out);
            match out.last_mut() {
                Some(Block::Tools(names)) => names.push(name),
                _ => out.push(Block::Tools(vec![name])),
            }
            start = pos + line.len();
        }
        pos += line.len();
    }
    markdown_blocks(&text[start..], &mut out);
    out
}

/// Splits Markdown at its top-level tables.
fn markdown_blocks<'a>(text: &'a str, out: &mut Vec<Block<'a>>) {
    let push_md = |out: &mut Vec<Block<'a>>, s: &'a str| {
        if !s.trim().is_empty() {
            out.push(Block::Markdown(s));
        }
    };
    let mut depth = 0;
    let mut last = 0;
    let mut table: Option<Table> = None;
    let mut cell: Option<usize> = None; // start offset of the cell being read
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        let step = match &event {
            Event::Start(_) => 1,
            Event::End(_) => -1,
            _ => 0,
        };
        match event {
            Event::Start(Tag::Table(align)) if depth == 0 => {
                push_md(out, &text[last..range.start]);
                table = Some(Table { align, header: Vec::new(), rows: Vec::new() });
            }
            Event::Start(Tag::TableRow) => table.iter_mut().for_each(|t| t.rows.push(Vec::new())),
            Event::Start(Tag::TableCell) => cell = Some(range.start),
            Event::End(TagEnd::TableCell) => {
                if let (Some(t), Some(s)) = (table.as_mut(), cell.take()) {
                    let src = text[s..range.end].trim().trim_matches('|').trim().to_owned();
                    match t.rows.last_mut() {
                        Some(row) => row.push(src),
                        None => t.header.push(src),
                    }
                }
            }
            Event::End(TagEnd::Table) if depth == 1 => {
                if let Some(mut t) = table.take() {
                    for row in &mut t.rows {
                        row.resize(t.header.len(), String::new());
                    }
                    out.push(Block::Table(t));
                }
                last = range.end;
            }
            _ => {}
        }
        depth += step;
    }
    push_md(out, &text[last..]);
}

/// Renders a message: Markdown via egui_commonmark, tables and tool calls by sessman.
pub fn show(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    for (i, block) in blocks(text).into_iter().enumerate() {
        ui.push_id(i, |ui| match block {
            Block::Markdown(src) => {
                CommonMarkViewer::new().show(ui, cache, &linkify(src));
            }
            Block::Table(t) => {
                ui.add_space(4.0);
                egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| table(ui, &t));
                ui.add_space(4.0);
            }
            Block::Tools(_) if !SHOW_TOOLS.load(Ordering::Relaxed) => {}
            Block::Tools(names) => {
                let text = format!("⚙ {}", names.join(" · "));
                ui.label(egui::RichText::new(text).monospace().small().weak());
            }
        });
    }
}

const CELL_PAD: egui::Vec2 = egui::vec2(8.0, 4.0);
const MAX_COL_WIDTH: f32 = 380.0;

/// A bordered table with a bold header, striped rows and aligned columns, drawn by hand so
/// columns size to their content and wide tables scroll instead of squeezing.
fn table(ui: &mut egui::Ui, t: &Table) {
    let rows: Vec<Vec<(std::sync::Arc<egui::Galley>, Option<String>)>> = std::iter::once(&t.header)
        .chain(&t.rows)
        .enumerate()
        .map(|(r, row)| row.iter().map(|c| inline(ui, c, r == 0)).collect())
        .collect();
    let cols = t.header.len();
    let widths: Vec<f32> = (0..cols)
        .map(|c| rows.iter().map(|row| row[c].0.size().x).fold(0.0, f32::max) + 2.0 * CELL_PAD.x)
        .collect();
    let total = widths.iter().sum::<f32>();
    let v = ui.visuals().clone();
    let border = Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color);
    let top = ui.cursor().top();
    let left = ui.cursor().left();
    for (r, row) in rows.iter().enumerate() {
        let height = row.iter().map(|(g, _)| g.size().y).fold(0.0, f32::max) + 2.0 * CELL_PAD.y;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(total, height), Sense::hover());
        let painter = ui.painter();
        if r == 0 {
            painter.rect_filled(rect, 0.0, v.code_bg_color);
        } else if r % 2 == 0 {
            painter.rect_filled(rect, 0.0, v.faint_bg_color);
        }
        let mut x = rect.left();
        for (c, (galley, link)) in row.iter().enumerate() {
            let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(widths[c], height));
            let free = widths[c] - 2.0 * CELL_PAD.x - galley.size().x;
            let dx = match t.align.get(c) {
                Some(Alignment::Right) => free,
                Some(Alignment::Center) => free / 2.0,
                _ => 0.0,
            };
            painter.galley(cell.min + CELL_PAD + egui::vec2(dx, 0.0), galley.clone(), v.text_color());
            if c > 0 {
                painter.vline(x, rect.y_range(), border);
            }
            if let Some(url) = link {
                let resp = ui.interact(cell, ui.id().with((r, c)), Sense::click()).on_hover_text(url);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
            x += widths[c];
        }
        if r == 0 {
            ui.painter().hline(rect.x_range(), rect.bottom(), border);
        }
    }
    let outline = egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(left + total, ui.cursor().top()));
    ui.painter().rect_stroke(outline, 2.0, border, egui::StrokeKind::Inside);
}

/// Lays out a cell's inline Markdown (bold, italic, code, strikethrough, links); returns the
/// first link so the cell can open it.
fn inline(ui: &egui::Ui, src: &str, header: bool) -> (std::sync::Arc<egui::Galley>, Option<String>) {
    let v = ui.visuals();
    let body = TextStyle::Body.resolve(ui.style());
    let mono = FontId::monospace(body.size * 0.9);
    let mut job = LayoutJob::default();
    job.wrap.max_width = MAX_COL_WIDTH;
    let (mut strong, mut italic, mut strike, mut link) = (header, false, false, None::<String>);
    let mut first_link = None;
    let fmt = |strong: bool, italic: bool, strike: bool, link: bool, code: bool| {
        let mut f = TextFormat::simple(if code { mono.clone() } else { body.clone() }, if link {
            v.hyperlink_color
        } else if strong {
            v.strong_text_color()
        } else {
            v.text_color()
        });
        f.italics = italic;
        if strike {
            f.strikethrough = Stroke::new(1.0, f.color);
        }
        if link {
            f.underline = Stroke::new(1.0, f.color);
        }
        if code {
            f.background = v.code_bg_color;
        }
        f.valign = Align::Center;
        f
    };
    let src = linkify(src);
    for event in Parser::new_ext(&src, options()) {
        match event {
            Event::Start(Tag::Strong) => strong = true,
            Event::End(TagEnd::Strong) => strong = header,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::Strikethrough) => strike = true,
            Event::End(TagEnd::Strikethrough) => strike = false,
            Event::Start(Tag::Link { dest_url, .. }) => {
                first_link.get_or_insert_with(|| dest_url.to_string());
                link = Some(dest_url.to_string());
            }
            Event::End(TagEnd::Link) => link = None,
            Event::Text(s) | Event::Html(s) | Event::InlineHtml(s) => {
                job.append(&s, 0.0, fmt(strong, italic, strike, link.is_some(), false))
            }
            Event::Code(s) => job.append(&s, 0.0, fmt(strong, italic, strike, link.is_some(), true)),
            Event::SoftBreak | Event::HardBreak => job.append(" ", 0.0, fmt(strong, italic, strike, false, false)),
            _ => {}
        }
    }
    if job.text.is_empty() {
        job.append(" ", 0.0, fmt(false, false, false, false, false));
    }
    (ui.painter().layout_job(job), first_link)
}

/// Bare URLs (`https://…`, `http://…`, `www.…`) in plain text become Markdown autolinks, so
/// they render as clickable links. Code, existing links and HTML are left alone; trailing
/// punctuation stays outside the link.
pub fn linkify(text: &str) -> Cow<'_, str> {
    // Byte ranges of plain text, adjacent text events merged (the parser splits at `_`, `*`…).
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut skip = 0; // inside a link, image or code block
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_)) => skip += 1,
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock) => skip -= 1,
            Event::Text(t) if skip == 0 && text.get(range.clone()) == Some(&*t) => match ranges.last_mut() {
                Some(last) if last.end == range.start => last.end = range.end,
                _ => ranges.push(range),
            },
            _ => {}
        }
    }
    let mut out = String::new();
    let mut done = 0;
    for r in ranges {
        let mut i = r.start;
        while i < r.end {
            let rest = &text[i..r.end];
            let starts = ["https://", "http://", "www."].iter().any(|p| rest.len() > p.len() && rest.get(..p.len()).is_some_and(|h| h.eq_ignore_ascii_case(p)));
            let boundary = text[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && c != '/' && c != '@' && c != '.');
            if !(starts && boundary) {
                i += rest.chars().next().map_or(1, char::len_utf8);
                continue;
            }
            let len = url_len(rest);
            if len == 0 {
                i += 1;
                continue;
            }
            let url = &rest[..len];
            out.push_str(&text[done..i]);
            if url.to_ascii_lowercase().starts_with("www.") {
                out.push_str(&format!("[{url}](https://{url})"));
            } else {
                out.push_str(&format!("<{url}>"));
            }
            i += len;
            done = i;
        }
    }
    if done == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[done..]);
    Cow::Owned(out)
}

/// Length of the URL at the start of `s`: up to whitespace or a delimiter, minus trailing
/// punctuation and unbalanced closing brackets.
fn url_len(s: &str) -> usize {
    let mut end = s.find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`' | '|')).unwrap_or(s.len());
    loop {
        let url = &s[..end];
        let Some(last) = url.chars().next_back() else { return 0 };
        let unbalanced = |open: char, close: char| last == close && url.matches(open).count() < url.matches(close).count();
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*' | '_' | '~') || unbalanced('(', ')') || unbalanced('[', ']') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    // Just a scheme or "www." is not a link.
    let url = &s[..end];
    if url.ends_with("://") || url.eq_ignore_ascii_case("www.") { 0 } else { end }
}

pub fn is_markdown(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_tables_with_alignment_and_pads_ragged_rows() {
        let text = "Intro **text**\n\n| Name | Count | Note |\n|:-----|------:|:----:|\n| a | 1 | x |\n| b | 2 |\n\nAfter.\n";
        let b = blocks(text);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0], Block::Markdown("Intro **text**\n\n"));
        let Block::Table(t) = &b[1] else { panic!("expected a table: {b:?}") };
        assert_eq!(t.align, vec![Alignment::Left, Alignment::Right, Alignment::Center]);
        assert_eq!(t.header, vec!["Name", "Count", "Note"]);
        assert_eq!(t.rows, vec![vec!["a", "1", "x"], vec!["b", "2", ""]]);
        assert_eq!(b[2], Block::Markdown("\nAfter.\n"));
    }

    #[test]
    fn keeps_inline_markdown_in_cells() {
        let b = blocks("| A |\n|---|\n| **bold** `x` |\n");
        let Block::Table(t) = &b[0] else { panic!() };
        assert_eq!(t.rows[0][0], "**bold** `x`");
    }

    #[test]
    fn bare_urls_become_links() {
        assert_eq!(linkify("See https://example.com/a_b?x=1."), "See <https://example.com/a_b?x=1>.");
        assert_eq!(linkify("(docs at https://x.dev/page) ok"), "(docs at <https://x.dev/page>) ok");
        assert_eq!(linkify("https://en.wikipedia.org/wiki/Rust_(language)"), "<https://en.wikipedia.org/wiki/Rust_(language)>");
        assert_eq!(linkify("go to www.rust-lang.org, now"), "go to [www.rust-lang.org](https://www.rust-lang.org), now");
        assert_eq!(linkify("- item http://a.io\n> quote https://b.io"), "- item <http://a.io>\n> quote <https://b.io>");
        assert_eq!(linkify("| a | https://t.io |"), "| a | <https://t.io> |");
    }

    /// Regression: multi-byte characters near the start of text made linkify panic.
    #[test]
    fn non_ascii_text_is_safe() {
        assert_eq!(linkify("a – b — c ✓ ünïcödé 日本語"), "a – b — c ✓ ünïcödé 日本語");
        assert_eq!(linkify("é https://x.io é"), "é <https://x.io> é");
        assert_eq!(linkify("–https://x.io"), "–<https://x.io>");
        assert_eq!(linkify("see https://x.io/ü–path."), "see <https://x.io/ü–path>.");
    }

    #[test]
    fn code_and_existing_links_stay_as_they_are() {
        for text in [
            "`https://in.code`",
            "```\nhttps://in.fence\n```",
            "[label](https://x.io)",
            "[https://shown.io](https://x.io)",
            "<https://already.io>",
            "no links here",
            "mailto:me@www.example.com",
            "https:// alone",
        ] {
            assert_eq!(linkify(text), text, "{text}");
        }
    }

    #[test]
    fn groups_tool_lines() {
        let b = blocks("Let me look.\n[tool: Read]\n[tool: Edit]\nDone.");
        assert_eq!(b, vec![Block::Markdown("Let me look.\n"), Block::Tools(vec!["Read", "Edit"]), Block::Markdown("Done.")]);
    }

    #[test]
    fn tables_in_code_fences_and_lists_stay_markdown() {
        let fenced = "```\n| a | b |\n|---|---|\n```\n";
        assert_eq!(blocks(fenced), vec![Block::Markdown(fenced)]);
        let unclosed = "```md\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        assert_eq!(blocks(unclosed), vec![Block::Markdown(unclosed)]);
        let quoted = "> | a |\n> |---|\n> | 1 |\n";
        assert_eq!(blocks(quoted), vec![Block::Markdown(quoted)]);
    }
}
