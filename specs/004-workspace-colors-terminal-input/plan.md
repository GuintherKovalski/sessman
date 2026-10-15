# Implementation Plan: Workspace Colors and Terminal-Style Input

**Branch**: `004-workspace-colors-terminal-input` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Summary

- `Workspace` gains `color: Option<[u8; 3]>` (serde default so old files load). `add` and
  `load` assign the first color from a fixed 10-color palette not used by another workspace;
  if all are taken, a golden-angle hue is generated. Pure logic, unit-tested.
- Sidebar: tabs and rows paint a tinted background (placeholder shape set after the widget is
  laid out, so it sits behind the text) and a 3 px stripe on the row's left edge.
- Main area: the input moves into a nested `Panel::bottom` inside the central panel, drawn in a
  near-black `Frame` with a `>` prompt and a frameless monospace `TextEdit`. The Send button is
  removed; a spinner shows while Claude runs.

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: fixed palette, no color picker |
| II. Don't own Claude's data | PASS: colors stored in workspaces.json |
| III. Single binary | PASS: no new dependency |
| IV. Learn by using | PASS |
