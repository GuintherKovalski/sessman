# Implementation Plan: Changes by File, File List, and Read-Only File Tabs

**Branch**: `009-changes-by-file-file-tabs` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Design

- New `diffview.rs`: `group(changes)` → per-file edits and counts; `lines(change)` → hunks via
  `similar::TextDiff::grouped_ops(3)` with `Gap` markers (both unit-tested); `file_card` renders a
  card (header click / double-click events); `diff_line` builds a `LayoutJob` from a sign plus
  the syntax-highlighted line, with a full-width tinted rect painted behind it; `file_view`
  renders line numbers + the highlighted file as a selectable, non-editable label; `read` refuses
  >2 MB or non-UTF-8 files.
- Syntax highlighting: `egui_extras` with the `syntect` feature (`highlight`, memoized, language
  from the file extension).
- `app.rs`: `changes_filter`, `file_tabs: Vec<(path, contents)>`, `main_tab`. The Changes tab
  gets a resizable bottom panel with the file list; cards scroll both ways so long lines don't
  widen the panel. The central panel gets a top tab bar when files are open. File contents
  reload whenever the session reloads.

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: read-only viewer, no editing, no search |
| II. Don't own Claude's data | PASS: files are only read |
| III. Single binary | PASS with justification: `egui_extras`/`syntect` added; hand-written highlighting would be far more code |
| IV. Learn by using | PASS |
