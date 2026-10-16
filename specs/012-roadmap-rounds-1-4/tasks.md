# Tasks: Roadmap Rounds 1–4

Tests only for pure logic (constitution); every item was also checked on screen in a sandboxed
copy of the app (`SESSMAN_CONFIG`), which was quit and reopened after each item.

- [x] T001 `settings.rs`: config folder (+ `$SESSMAN_CONFIG`), typed settings, page, search, reset; tests
- [x] T002 `queue.rs`, `state.rs`, `refine.rs` with tests
- [x] T003 Item 1: `linkify` (+ regression test for non-ASCII text, which crashed on screen)
- [x] T004 Item 2: `closable_tab`, middle-click, `after_close` test
- [x] T005 Item 3: growing input (measured text height), drag handle, collapse triangle, `input_heights` test; fixed the prompt panel / input id clash
- [x] T006 Item 4: transparent title bar + top bar (drag, zoom); viewport test
- [x] T007 Item 5: native folder picker, pending new-session rows, `pending_new` test
- [x] T008 Item 6: per-session views; `Views::switch` test
- [x] T009 Item 7: last session per workspace, delete keeps the right tab; tests
- [x] T010 Item 8: state save/restore, eframe persistence, broken-file handling; tests
- [x] T011 Item 9: Ctrl+C / ⏹ stop with process groups; interrupt marker shown once; tests (incl. a real process-group stop)
- [x] T012 Item 10: queue UI, auto-send, pause/resume, edit keeps the pause
- [x] T013 Item 11: Settings tab, Cmd+,, apply + save, confirm-delete modal, notifications
- [x] T014 Item 12: Refine (button, Cmd+Enter), questions with Other/Skip/Skip all, undo, Esc; focus fix for Other fields
- [x] T015 `cargo test`, `cargo clippy` (no new warnings), PROJECT.md, README.md (Gaps), ROADMAP.md statuses
