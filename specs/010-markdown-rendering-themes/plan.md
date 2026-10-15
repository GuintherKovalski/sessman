# Implementation Plan: Markdown Rendering, Styling, and Themes

**Branch**: `010-markdown-rendering-themes` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Technical Context

Rust 2024, egui/eframe 0.36, `egui_extras` (syntect) already present. Adds `egui_commonmark`
0.25 (built for egui 0.36), without default features (no image loading), with
`pulldown_cmark` + `better_syntax_highlighting` (syntect, already compiled for diffs).
See [research.md](research.md) for the choices.

## Design

- New `markdown.rs`:
  - `blocks(text) -> Vec<Block>`: splits a message into `Markdown(&str)`, `Table(Table)` and
    `Tool(&str)` blocks. Lines starting with `[tool:` (how `session.rs` records tool calls)
    become `Tool`; top-level tables (found with `pulldown_cmark`'s offset iterator) become
    `Table { align, header, rows }` with cell text; everything else stays Markdown source.
    Pure, unit-tested (tables, ragged rows, tool lines, unclosed fences).
  - `show(ui, cache, id, text)`: renders the blocks. Markdown goes to
    `egui_commonmark::CommonMarkViewer` (headings, emphasis, lists, quotes, links, task lists,
    code blocks with highlighting; inline/blocked HTML shown as text). Tables are drawn by
    sessman, painted by hand inside a horizontal `ScrollArea` (a `Grid` can't align cells
    reliably): columns sized to their content (max 380 px, then wrap), bold header row, striped
    rows, borders, cells placed right / center / left per the alignment row; cell text goes
    through a small inline formatter (`**bold**`, `*italic*`, `` `code` ``, `~~strike~~`,
    links in link color, clickable). Tool blocks: one small, weak, monospace line.
- `app.rs`:
  - Conversation: each message is a card (`Frame`, faint fill, rounded, left accent stripe in
    the role color, role label above), rendered by `markdown::show`. One `CommonMarkCache` in
    `App`.
  - Smooth scrolling (SC-004): message heights are remembered (`Vec<f32>`, reset when the
    session reloads or the width changes); messages fully outside the viewport only
    `allocate_space` their remembered height instead of rendering. The pinned-prompt logic
    keeps using cursor positions, so it still works.
  - Pinned "You": rendered with `markdown::show` inside its 90 px scroll area.
  - File tabs: `file_tabs` entries gain `rendered: bool` (true for `.md`/`.markdown`). The
    file tab shows a `Rendered | Source` toggle for Markdown files; Rendered uses
    `markdown::show` in a scroll area, Source is today's `diffview::file_view`.
  - Theme: `ThemePreference` (Dark / Light / System) set with `ctx.set_theme`; egui follows
    macOS for System. A small three-way switch (☀ / ☾ / System) at the top-right of the left
    panel header. Saved as `{"theme": "dark|light|system"}` in
    `~/.config/sessman/settings.json` (the existing `Names` string map, new path). Default:
    System.
- Colors (FR-007): a `palette(visuals)` helper returns the role accents, prompt-input fill /
  text / prompt-sign color, diff add/del fg+bg, line-number and border grays, with a dark and
  a light set. Replaces the hard-coded `Color32` constants in `app.rs` and `diffview.rs`.
  Syntax themes already switch with `dark_mode` (`CodeTheme::from_memory`,
  CommonMark `syntax_theme_light/dark`). Workspace colors keep their hue; tints use alpha so
  they work on both backgrounds.

## Data

| Thing | Where | Shape |
|-------|-------|-------|
| Theme choice | `~/.config/sessman/settings.json` | `{"theme": "system"}` |
| File tab | `App::file_tabs` | `(path, contents, rendered: bool)` |
| Message heights | `App` (memory only) | `Vec<f32>` + width they were measured at |

No contracts: no external interface changes.

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: three themes, no custom colors/fonts; tables only special-cased where the library falls short |
| II. Don't own Claude's data | PASS: rendering only; theme saved in sessman's own config folder |
| III. Single binary | PASS with justification: `egui_commonmark` replaces a hand-written Markdown renderer (lists, quotes, code, links, nesting) that would be several hundred lines |
| IV. Learn by using | PASS: switch placement and card styling are first guesses, tuned by use |

Post-design re-check: unchanged, PASS.

## Project Structure

```
src/
├── markdown.rs   NEW: block splitting (tested), Markdown + table rendering
├── theme.rs      NEW: theme preference load/save, palette for dark/light
├── app.rs        message cards, culling, pinned prompt, file-tab toggle, theme switch
└── diffview.rs   palette colors instead of constants
```
