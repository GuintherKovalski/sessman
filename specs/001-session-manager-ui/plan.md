# Implementation Plan: Claude Session Manager UI

**Branch**: `001-session-manager-ui` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-session-manager-ui/spec.md`

## Summary

A native desktop window in three columns: session list (left), conversation with a pinned
prompt bar (center), code changes (right). Sessions are read from Claude Code's JSONL
transcripts in `~/.claude/projects/*/*.jsonl`. Prompts are sent by spawning
`claude -p --resume <id> "<prompt>"` in the session's working directory on a background
thread; when it exits the session file is re-read. Diffs come from `Edit`/`MultiEdit`/`Write`
tool calls recorded in the transcript.

## Technical Context

**Language/Version**: Rust (stable, edition 2021)

**Primary Dependencies**: `eframe`/`egui` (immediate-mode GUI, mouse + keyboard, one crate), `serde_json` (transcript parsing), `similar` (line diffs)

**Storage**: None of our own; reads `~/.claude/projects` (read-only)

**Testing**: `cargo test` for transcript parsing and prompt-history navigation

**Target Platform**: macOS desktop (egui is cross-platform, not a goal)

**Project Type**: desktop-app

**Performance Goals**: Sidebar ready < 2 s with 200+ sessions; open session < 1 s

**Constraints**: Never write to Claude's files; UI never blocks on `claude`

**Scale/Scope**: One user, hundreds of sessions, ~600 lines of Rust

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: no streaming, no embedded terminal, no settings, manual refresh |
| II. Read Claude's data, don't own it | PASS: transcripts are read-only, prompts go through the `claude` CLI |
| III. Single binary | PASS: one crate, three dependencies |
| IV. Learn by using | PASS: open questions listed in research.md |

Post-design re-check: PASS (no new projects, layers, or persistence introduced).

## Project Structure

### Documentation (this feature)

```text
specs/001-session-manager-ui/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/ui.md
└── tasks.md
```

### Source Code (repository root)

```text
Cargo.toml
src/
├── main.rs       # window setup
├── app.rs        # egui layout: left / top prompt / center / right panels
├── session.rs    # discover + parse JSONL transcripts → Session, Message, Change
├── history.rs    # Up/Down prompt-history cursor (pure, unit-tested)
└── runner.rs     # spawn `claude -p --resume` on a thread, report completion
```

**Structure Decision**: Single crate, flat modules. Tests live as `#[cfg(test)]` modules next
to the code they test.

## Complexity Tracking

None.
