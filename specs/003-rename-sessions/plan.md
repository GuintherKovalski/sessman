# Implementation Plan: Rename Sessions

**Branch**: `003-rename-sessions` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Summary

`names.rs` keeps a `session id → name` map saved to `~/.config/sessman/names.json`. In the
sidebar, `double_clicked()` on a row sets `renaming = (id, text)`; that row renders a
`TextEdit` instead of the label, focused, with the whole text selected on the first frame via
`TextEditState::cursor.set_char_range`. `lost_focus()` ends the edit: Escape cancels,
anything else (Enter, click away) saves.

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: one small module, no new dependency |
| II. Don't own Claude's data | PASS: names stored in sessman's config, not in session files |
| III. Single binary | PASS |
| IV. Learn by using | PASS |

## Project Structure

```text
src/
├── names.rs   # NEW: custom names map, set/get/load/save (unit-tested)
└── app.rs     # inline rename in the session list; titles go through names
```
