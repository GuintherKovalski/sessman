# Tasks: Workspaces and Pinned Prompt

## Phase 1: Foundational

- [x] T001 Add `serde` with derive to Cargo.toml
- [x] T002 [P] `Workspaces` model with add/move_to/remove/delete/load/save + unit tests in src/workspaces.rs

## Phase 2: User Story 1 - Pinned prompt (P1)

- [x] T003 [US1] Track user-message offsets with `show_viewport` and render the pinned "You" box in src/app.rs

## Phase 3: User Story 2 - Workspace tabs (P1)

- [x] T004 [US2] Tab row (All sessions, workspaces, +) with inline name field; filter list by tab in src/app.rs
- [x] T005 [US2] Right-click a tab → Delete workspace in src/app.rs

## Phase 4: User Story 3 - Move sessions (P1)

- [x] T006 [US3] Session rows as drag sources; tabs accept drops and highlight on hover in src/app.rs
- [x] T007 [US3] Session context menu: Move to → workspaces, Remove from workspace in src/app.rs

## Dependencies

T001 → T002 → T004 → T005–T007. T003 is independent.
