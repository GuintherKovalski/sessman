# Implementation Plan: Workspaces and Pinned Prompt

**Branch**: `002-workspaces-pinned-prompt` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Summary

Pinned prompt: render the conversation with `ScrollArea::show_viewport`, record each user
message's top offset, and pick the last one at or above the viewport top; draw it in a framed
box above the scroll area (one frame of lag is invisible). Workspaces: a `workspaces.rs`
module holding `Vec<Workspace { name, sessions }>` saved as JSON to
`~/.config/sessman/workspaces.json`; tabs via `horizontal_wrapped` selectable labels; drag
via `ui.dnd_drag_source` on session rows and `Response::dnd_release_payload` on tabs;
right-click via `Response::context_menu` + `menu_button("Move to")`.

## Technical Context

**Language/Version**: Rust 2024 · **Primary Dependencies**: eframe/egui 0.36, serde (derive) + serde_json · **Storage**: `~/.config/sessman/workspaces.json` · **Testing**: cargo test for workspace operations · **Project Type**: desktop-app

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: no rename/reorder, no nested workspaces |
| II. Don't own Claude's data | PASS: workspaces are sessman's own data, stored outside `~/.claude` |
| III. Single binary | PASS: adds `serde` derive only (already a transitive dependency) |
| IV. Learn by using | PASS |

## Project Structure

```text
src/
├── workspaces.rs   # NEW: Workspace list, move/remove/delete, load/save (unit-tested)
└── app.rs          # tabs, drag-and-drop, context menu, pinned prompt
```
