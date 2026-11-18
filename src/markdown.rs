use eframe::egui::{self, Align, FontId, Sense, Stroke, TextFormat, TextStyle};
use eframe::egui::text::LayoutJob;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

/// The "Show tool lines" setting.
pub static SHOW_TOOLS: AtomicBool = AtomicBool::new(true);

/// A message split into what scope draws itself (tables, tool calls) and plain Markdown.
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

/// Renders a message: Markdown via egui_commonmark, tables and tool calls by scope.
pub fn show(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    for (i, block) in blocks(text).into_iter().enumerate() {
        ui.push_id(i, |ui| match block {
            Block::Markdown(src) => {
                CommonMarkViewer::new().show(ui, cache, &linkify(src));
            }
            Block::Table(t) => {
                ui.add_space(4.0);
                let out = egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| table(ui, &t));
                let (outline, rows) = out.inner;
                // Copy, at the top right of the table's visible part while the pointer is on it.
                let visible = outline.intersect(out.inner_rect);
                if visible.width() > 30.0 && ui.rect_contains_pointer(visible) {
                    let at = egui::Rect::from_min_size(visible.right_top() + egui::vec2(-24.0, 3.0), egui::vec2(20.0, 18.0));
                    let fill = ui.visuals().code_bg_color;
                    ui.painter().rect_filled(at.expand(1.0), 4.0, fill);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
                        copy_button(ui, || tsv(&rows), "Copy the table (tab-separated, to paste into a spreadsheet)", fill);
                    });
                }
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

/// Copy at the right end of a message's label row (and on side questions, and tables): two
/// overlapping rounded squares, drawn (no font has them), then a check mark for a moment after
/// copying.
/// `text` is what goes on the clipboard. `fill` is the card's background, which the front
/// square is filled with so it covers the back one.
pub fn copy_button(ui: &mut egui::Ui, text: impl FnOnce() -> String, hover: &str, fill: egui::Color32) {
    let copied_id = ui.id().with("copied");
    let now = ui.input(|i| i.time);
    let copied = ui.data(|d| d.get_temp::<f64>(copied_id)).is_some_and(|t| now - t < 1.5);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 18.0), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Copy"));
    let resp = resp.on_hover_text(hover).on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() {
        ui.ctx().copy_text(text());
        ui.data_mut(|d| d.insert_temp(copied_id, now));
    }
    let v = ui.visuals();
    let bg = if resp.hovered() { v.widgets.hovered.weak_bg_fill } else { fill };
    let color = if resp.hovered() { v.strong_text_color() } else { v.weak_text_color() };
    let stroke = egui::Stroke::new(1.3, color);
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 4.0, bg);
    }
    let c = rect.center();
    if copied {
        let pts = vec![c + egui::vec2(-4.5, 0.0), c + egui::vec2(-1.5, 3.0), c + egui::vec2(4.5, -3.5)];
        p.add(egui::Shape::line(pts, stroke));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
    } else {
        let (side, shift) = (9.0, 3.5);
        let back = egui::Rect::from_min_size(c + egui::vec2(-side / 2.0 - shift / 2.0 + shift, -side / 2.0 - shift / 2.0), egui::vec2(side, side));
        let front = back.translate(egui::vec2(-shift, shift));
        p.rect_stroke(back, 1.5, stroke, egui::StrokeKind::Middle);
        p.rect_filled(front, 1.5, bg);
        p.rect_stroke(front, 1.5, stroke, egui::StrokeKind::Middle);
    }
}

/// `text` with the characters Markdown would read as formatting escaped, to put it inside a
/// link's text as it is.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '[' | ']' | '*' | '_' | '`' | '<' | '>' | '#' | '~' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Takes back a link clicked this frame whose URL starts with `prefix` (an app link, not a web
/// page), so it is not opened in the browser; returns what follows the prefix.
pub fn take_url(ctx: &egui::Context, prefix: &str) -> Option<String> {
    ctx.output_mut(|o| {
        let mut found = None;
        o.commands.retain(|c| match c {
            egui::OutputCommand::OpenUrl(u) if u.url.starts_with(prefix) => {
                found = Some(u.url[prefix.len()..].to_owned());
                false
            }
            _ => true,
        });
        found
    })
}

const CELL_PAD: egui::Vec2 = egui::vec2(8.0, 4.0);
const MAX_COL_WIDTH: f32 = 380.0;

/// A bordered table with a bold header, striped rows and aligned columns, drawn by hand so
/// columns size to their content and wide tables scroll instead of squeezing. Cell text is
/// selectable (drag across cells, Cmd+C), a cell's right-click menu copies it or the table, and
/// so does Copy at its top right on hover. Returns its outline and cells, for that button.
fn table(ui: &mut egui::Ui, t: &Table) -> (egui::Rect, Cells) {
    let rows: Cells = std::iter::once(&t.header)
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
        let painter = ui.painter().clone();
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
            if c > 0 {
                painter.vline(x, rect.y_range(), border);
            }
            let at = egui::Rect::from_min_size(cell.min + CELL_PAD + egui::vec2(dx, 0.0), galley.size());
            // `place`, not `put`: `put` moves the cursor below the cell, so the next row would
            // start under the last cell instead of under the row's tallest one.
            let mut resp = ui.place(at, egui::Label::new(galley.clone()).selectable(true));
            if let Some(url) = link {
                resp = resp.on_hover_text(url);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
            resp.context_menu(|ui| {
                if ui.button("Copy cell").clicked() {
                    ui.ctx().copy_text(galley.text().to_owned());
                }
                if ui.button("Copy table").on_hover_text("Tab-separated, to paste into a spreadsheet").clicked() {
                    ui.ctx().copy_text(tsv(&rows));
                }
                if let Some(url) = link
                    && ui.button("Copy link").clicked()
                {
                    ui.ctx().copy_text(url.clone());
                }
            });
            x += widths[c];
        }
        if r == 0 {
            ui.painter().hline(rect.x_range(), rect.bottom(), border);
        }
    }
    let outline = egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(left + total, ui.cursor().top()));
    ui.painter().rect_stroke(outline, 2.0, border, egui::StrokeKind::Inside);
    (outline, rows)
}

/// A table's cells, header row first: laid out text and the first link.
type Cells = Vec<Vec<(std::sync::Arc<egui::Galley>, Option<String>)>>;

/// The table's text as tab-separated rows, header first.
fn tsv(rows: &Cells) -> String {
    let line = |row: &Vec<(std::sync::Arc<egui::Galley>, Option<String>)>| {
        row.iter().map(|(g, _)| g.text().trim().replace(['\t', '\n'], " ")).collect::<Vec<_>>().join("\t")
    };
    rows.iter().map(line).collect::<Vec<_>>().join("\n")
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
    fn escaped_text_reads_as_it_is() {
        assert_eq!(escape("a [b] *c* d_e"), "a \\[b\\] \\*c\\* d\\_e");
        let md = format!("[{}](x:1)", escape("Fix [auth] *now*"));
        let text: String = Parser::new(&md).filter_map(|e| if let Event::Text(t) = e { Some(t.to_string()) } else { None }).collect();
        assert_eq!(text, "Fix [auth] *now*");
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

#[cfg(test)]
mod drag_tests {
    use super::*;

    /// Drags from the first header cell to the last body cell of a table and copies.
    #[test]
    fn dragging_across_a_table_selects_its_cells() {
        let ctx = egui::Context::default();
        let mut cache = CommonMarkCache::default();
        let text = "Before\n\n| Name | Value |\n|---|---|\n| alpha | 1 |\n| beta | 2 |\n\nAfter\n";
        let mut rects: Vec<egui::Rect> = Vec::new();
        let mut frame = |events: Vec<egui::Event>, rects: &mut Vec<egui::Rect>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let r = ui.scope(|ui| show(ui, &mut cache, text)).response.rect;
                rects.clear();
                rects.push(r);
            });
            out.textures_delta.clear();
            out
        };
        frame(vec![], &mut rects);
        frame(vec![], &mut rects);
        let r = rects[0];
        // Start left of "Before", end at the bottom right of the content.
        let start = r.left_top() + egui::vec2(1.0, 5.0);
        let end = r.left_bottom() + egui::vec2(30.0, -5.0);
        let press = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(vec![egui::Event::PointerMoved(start)], &mut rects);
        frame(vec![press(start, true)], &mut rects);
        for i in 1..=10 {
            let p = start + (end - start) * (i as f32 / 10.0);
            frame(vec![egui::Event::PointerMoved(p)], &mut rects);
        }
        frame(vec![press(end, false)], &mut rects);
        let out = frame(vec![egui::Event::Copy], &mut rects);
        let copied: Vec<_> = out.platform_output.commands.iter().filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        }).collect();
        let all = copied.join("");
        for word in ["Before", "Name", "alpha", "2", "After"] {
            assert!(all.contains(word), "missing {word} in {all:?}");
        }
    }
}

#[cfg(test)]
mod copy_tests {
    use super::*;

    /// Hovering a table shows Copy at its top right; clicking it copies the table as TSV.
    #[test]
    fn copy_button_copies_the_table() {
        let ctx = egui::Context::default();
        let mut cache = CommonMarkCache::default();
        let text = "| Name | Value |\n|---|---|\n| alpha | 1 |\n| **beta** | 2 |\n";
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| show(ui, &mut cache, text));
            out.textures_delta.clear();
            out
        };
        frame(vec![]);
        let out = frame(vec![]);
        // The table's outline: the bounding box of its cells' text, plus padding.
        let texts: Vec<egui::Rect> = out.shapes.iter().filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(egui::Rect::from_min_size(t.pos, t.galley.size())),
            _ => None,
        }).collect();
        let right = texts.iter().map(|r| r.right()).fold(0.0, f32::max) + CELL_PAD.x;
        let top = texts.iter().map(|r| r.top()).fold(f32::INFINITY, f32::min) - CELL_PAD.y;
        let button = egui::pos2(right - 14.0, top + 12.0);
        let press = |pressed| egui::Event::PointerButton { pos: button, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(vec![egui::Event::PointerMoved(button)]);
        frame(vec![]);
        frame(vec![press(true)]);
        let out = frame(vec![press(false)]);
        let copied: Vec<_> = out.platform_output.commands.iter().filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        }).collect();
        assert_eq!(copied, vec!["Name\tValue\nalpha\t1\nbeta\t2".to_owned()]);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    /// A cell that wraps to several lines, in a row whose last cell is one line, pushes the
    /// next row down: no cell's text overlaps another's.
    #[test]
    fn wrapped_cells_push_the_next_row_down() {
        let ctx = egui::Context::default();
        let mut cache = CommonMarkCache::default();
        let long = "a helper model rewrites the prompt clearly, asking questions if it is ambiguous, and more words to wrap";
        let text = format!("| A | B | C |\n|---|---|---|\n| x | {long} {long} | y |\n| x | short | y |\n| x | {long} | y |\n\nAfter\n");
        let mut rects = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| show(ui, &mut cache, &text));
            out.textures_delta.clear();
            rects = out.shapes.iter().filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => Some((t.galley.job.text.clone(), egui::Rect::from_min_size(t.pos, t.galley.size()))),
                _ => None,
            }).collect();
        }
        assert!(rects.iter().any(|(_, r)| r.height() > 30.0), "expected a wrapped cell: {rects:?}");
        for (i, (a, ra)) in rects.iter().enumerate() {
            for (b, rb) in &rects[i + 1..] {
                assert!(!ra.intersects(*rb), "{a:?} at {ra:?} overlaps {b:?} at {rb:?}");
            }
        }
    }
}
