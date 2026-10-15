# Tasks: Session Search, Live Progress, Multi-line Prompt

Tests only for pure logic (constitution): `Session::apply`, scoring, ranking, snippets, AI reply parsing.

## Phase 1: Foundational

- [x] T001 [P] Split `parse` into `Session::apply(&mut self, &Value)` + `Session::push_prompt(text)`; test that applying lines one by one equals `parse`, in src/session.rs
- [x] T002 [P] `search.rs`: `score`, `content_hit` (snippet with byte offsets from ASCII-lowercased text), `rank` (name → goal → content, score, recency, each session once), `parse_ai`, `ai_prompt`, with tests; register module in src/main.rs

## Phase 2: US1 – Live progress (P1) 🎯 MVP

- [x] T003 [US1] `runner::send` streams: `--output-format stream-json --verbose --include-partial-messages`, read stdout lines, emit `RunEvent::{Line, Delta, Tool, Done}`; stderr / `result.is_error` → `Done(Err)`, in src/runner.rs
- [x] T004 [US1] `App`: push the prompt on send; `poll_runner` drains all events per frame (`Line` → `Session::apply` + clear `live`; `Delta` → append; `Tool` → note; `Done` → reload, refresh list, error); live Claude card with spinner at the end of the conversation; works for new sessions too, in src/app.rs

## Phase 3: US2 – Live search (P1)

- [x] T005 [US2] `search::Index` + `spawn_indexer(ctx)` thread (10 s, re-read changed files only) sending `Snapshot { sessions, index }`, in src/search.rs
- [x] T006 [US2] `App` takes snapshots (keep selection by id, as `refresh_list` does); search field in a bottom panel of the sidebar, Escape clears; non-empty query shows ranked hits within the active tab, with source/snippet as the row's second line; cache hits until query/snapshot changes, in src/app.rs

## Phase 4: US3 – AI search (P2)

- [x] T007 [US3] `runner::ask_haiku`; `summarize_goal` uses it, in src/runner.rs
- [x] T008 [US3] Enter in search → catalog of the 40 most recent visible sessions → `ask_haiku`; spinner; results with reasons; editing the query drops them; failure/none → message, in src/app.rs

## Phase 5: US4 – Multi-line prompt (P2)

- [x] T009 [US4] Multi-line `TextEdit` growing to ~10 lines then scrolling; plain Enter consumed → send; Shift+Enter → new line; Up/Down history only without newlines, in src/app.rs

## Phase 6: US5 – Header removed (P3)

- [x] T010 [US5] Remove heading and ⟳ (and `refresh_list` button use); theme switch right-aligned in the right panel tab bar, in src/app.rs

## Phase 7: Polish

- [x] T011 `cargo test`, `cargo clippy`, run the app
- [x] T012 Update PROJECT.md, README.md (incl. Gaps) and the specs table

## Dependencies

T001 → T003 → T004. T002 → T005 → T006 → T008 (needs T007). T009, T010 independent.
Parallel: T001 ∥ T002; T009 ∥ T010 ∥ US2.

## Strategy

MVP = US1 (live progress), then live search, AI search, multi-line input, header cleanup.
