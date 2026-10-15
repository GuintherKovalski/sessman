# Tasks: Markdown Rendering, Styling, and Themes

Tests only for pure logic (constitution): block splitting (settings use the already-tested `Names` map).

## Phase 1: Setup

- [x] T001 Add `egui_commonmark = { version = "0.25", default-features = false, features = ["pulldown_cmark", "better_syntax_highlighting"] }` and `pulldown-cmark` (same version it uses) to Cargo.toml; `cargo build`

## Phase 2: Foundational

- [x] T002 [P] `theme.rs`: `Theme` (Dark/Light/System) load/save via `Names` map at `~/.config/sessman/settings.json` (key `theme`, default System), `palette(&Visuals) -> Palette` with dark and light sets (user/assistant accents, prompt fill/text/sign, diff add/del fg+bg, line numbers, borders); register module in src/main.rs
- [x] T003 [P] `markdown.rs`: `blocks(text) -> Vec<Block>` (Markdown / Table{align, header, rows} / Tool) with tests for: table with alignment, ragged rows padded, `[tool:` lines, table inside an unclosed fence not split, text before/after a table kept, in src/markdown.rs

## Phase 3: US1 – Conversation rendered as Markdown (P1) 🎯 MVP

Independent test: a reply with a table and a code block renders formatted.

- [x] T004 [US1] `markdown::show(ui, cache, text)`: Markdown blocks through `CommonMarkViewer`; tables drawn by hand (columns sized to content, max 380 px with wrapping) in a horizontal `ScrollArea`, bordered, striped, bold header, per-column alignment, inline formatting (bold/italic/code/strike/links clickable); Tool blocks as one small weak monospace line, in src/markdown.rs
- [x] T005 [US1] Use `markdown::show` for conversation messages and the pinned "You" prompt; one `CommonMarkCache` in `App`, in src/app.rs
- [x] T006 [US1] Cull off-screen messages: remember heights (reset on reload/width change), `allocate_space` for messages outside the viewport, keep pinned-prompt detection working, in src/app.rs

## Phase 4: US2 – .md files open rendered (P1)

Independent test: double-click a changed `.md` file → rendered; Source shows line numbers.

- [x] T007 [US2] `file_tabs` gains `rendered: bool` (true for `.md`/`.markdown`); Markdown file tabs show a `Rendered | Source` toggle; Rendered uses `markdown::show` in a scroll area, Source uses `diffview::file_view`, in src/app.rs

## Phase 5: US3 – Themes (P2)

Independent test: switch to Light, everything readable; restart keeps it.

- [x] T008 [US3] Apply saved theme at startup (`ctx.set_theme`) and add the Dark / Light / System switch in the left panel header, saving on change, in src/app.rs
- [x] T009 [US3] Replace hard-coded colors with `theme::palette` in src/app.rs (prompt input, accents, error text) and src/diffview.rs (diff tints, line numbers, borders); set CommonMark syntax themes per mode

## Phase 6: US4 – Cleaner conversation styling (P2)

- [x] T010 [US4] Message cards: faint fill, rounded, left accent stripe in role color, role label, even spacing, in src/app.rs

## Phase 7: Polish

- [x] T011 `cargo test`, `cargo clippy`; app launches. Quickstart walk-through left to the user (not driven on the live desktop)
- [x] T012 Update PROJECT.md (window, how it works, code layout, specs table) and README.md

## Dependencies

T001 → T002, T003 → US1 (T004–T006) → US2 (T007, needs `markdown::show`). US3 (T008–T009) needs T002 only. US4 (T010) builds on T005 and T009's palette.
Parallel: T002 ∥ T003; US3 ∥ US1/US2 after T002.

## Strategy

MVP = US1 (Markdown + tables in the conversation). Then .md files, themes, styling; each one runnable on its own.
