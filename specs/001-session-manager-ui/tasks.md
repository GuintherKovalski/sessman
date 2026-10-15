# Tasks: Claude Session Manager UI

**Input**: Design documents from `/specs/001-session-manager-ui/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ui.md

**Tests**: Unit tests only for pure logic (constitution: Constraints).

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [x] T001 Create crate with `eframe`, `serde_json`, `similar` in Cargo.toml and `.gitignore` for `target/`
- [x] T002 Open an empty eframe window titled "sessman" in src/main.rs

## Phase 2: Foundational

- [x] T003 Discover transcripts under `~/.claude/projects/*/*.jsonl` into `SessionMeta` sorted by mtime in src/session.rs
- [x] T004 Parse a transcript into `Session` (messages, prompts, changes, cwd, title) skipping bad lines in src/session.rs
- [x] T005 [P] Unit tests for parsing string/array content, command wrappers, Edit/Write changes in src/session.rs

**Checkpoint**: data layer ready

## Phase 3: User Story 1 - Browse and open sessions (P1) 🎯 MVP

**Independent test**: launch, click a session, see messages.

- [x] T006 [US1] Left `SidePanel` with selectable session titles + Refresh button in src/app.rs
- [x] T007 [US1] Central scroll area rendering messages with You/Claude labels in src/app.rs

## Phase 4: User Story 2 - Prompt bar with history (P1)

**Independent test**: Up/Down cycles prompts; Enter sends and reply appears.

- [x] T008 [P] [US2] `History` with up/down/draft semantics + unit tests in src/history.rs
- [x] T009 [US2] `TopBottomPanel` prompt input pinned above the conversation, Up/Down wired to `History` in src/app.rs
- [x] T010 [US2] Background `claude -p --resume` runner with completion channel in src/runner.rs
- [x] T011 [US2] Enter sends, input disabled while running, reload session on completion, show errors in src/app.rs

## Phase 5: User Story 3 - Diff panel (P2)

**Independent test**: session with edits shows red/green diffs on the right.

- [x] T012 [US3] Right `SidePanel` rendering each `Change` as a line diff via `similar` in src/app.rs

## Phase 6: Polish

- [x] T013 README with run instructions and open questions

## Dependencies

Setup → Foundational → US1 → (US2, US3 independent of each other). T005 and T008 can run in parallel with UI work.

## Implementation Strategy

MVP = Phases 1–3. Then US2 (main feature #1), then US3. Stop after each phase and run the app.
