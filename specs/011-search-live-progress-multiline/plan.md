# Implementation Plan: Session Search, Live Progress, Multi-line Prompt

**Branch**: `011-search-live-progress-multiline` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Technical Context

Rust 2024, egui/eframe 0.36. No new dependencies. Live progress uses `claude -p
--output-format stream-json --verbose --include-partial-messages` (verified locally: emits
`stream_event` lines with `text_delta`s and `content_block_start` for tool_use, complete
`assistant` / `user` lines in the same shape as the session file, and a final `result`).
See [research.md](research.md).

## Design

### Live progress (US1)
- `session.rs`: the per-line body of `parse` becomes `Session::apply(&mut self, &Value)`;
  `parse` calls it. Streamed `assistant` / `user` lines have the session-file shape, so the
  open session is updated in place: text, tool lines, changes, model and context tokens all
  appear mid-run. New `Session::push_prompt(text)` adds the sent prompt right away.
- `runner.rs`: `send` returns `Receiver<RunEvent>`: `Line(Value)` for each complete
  `assistant`/`user` line, `Delta(String)` for text deltas, `Tool(String)` when a tool_use
  block starts, `Done(Result<(), String>)` at exit (stderr / `result.is_error` → Err). Reads
  stdout line by line on the thread and repaints per event.
- `app.rs`: `live: String`: the reply being written (deltas), cleared when its complete
  `assistant` line arrives. While running, the conversation ends with a Claude card showing
  `live` (Markdown) plus a spinner, or "⚙ Name…" while a tool starts. On `Done`, reload from
  disk as today (same content, so nothing jumps), refresh the list.

### Search (US2, US3)
- New `search.rs`:
  - `Index`: per session id → `{ mtime, text, lower, prompts }` (`text` = messages without
    tool lines, capped at 200 kB; `lower` = `to_ascii_lowercase`, so byte offsets match
    `text` for snippets; `prompts` = first 3 prompts, 150 chars each).
  - `spawn_indexer(ctx) -> Receiver<Snapshot>`: a thread that every 10 s lists sessions and
    re-reads only files whose mtime changed; sends `Snapshot { sessions, index }`. This is also
    the automatic list refresh (US5).
  - `score(query, text) -> Option<u32>`: 100 substring, 80 all words present, 50 every word
    within a small edit distance of a word in the text (1 edit for 4–7 letters, 2 for 8+),
    else None. Used for names and goals.
  - `content_hit(query, entry) -> Option<String>`: all words present (case-insensitive),
    query ≥ 2 chars; returns a ~80-char snippet around the first word.
  - `rank(query, candidates) -> Vec<Hit { index, source, snippet }>`: name, then goal, then
    content; within a source by score, then recency. Each session once.
  - `ai_prompt(query, catalog)` and `parse_ai(reply, n) -> Vec<(usize, reason)>` (numbered
    lines `N: reason`; unknown numbers and junk ignored).
  - All pure parts unit-tested.
- `runner.rs`: `ask_haiku(ctx, prompt)` generalizes `summarize_goal`'s command
  (`--model haiku --no-session-persistence`, temp dir).
- `app.rs` sidebar: bottom panel with a single-line search field (hint "Search · Enter asks
  Haiku", Escape clears). Non-empty query → the list shows ranked hits (second row line:
  "goal" / snippet / AI reason instead of the folder). Enter → AI results replace live ones
  with a spinner until done; editing the query drops them. Search runs only when the query or
  snapshot changes (cached hits).

### Prompt input (US4)
- `TextEdit::multiline`, `desired_rows(1)`, grows with content; wrapped in a `ScrollArea` with
  `max_height` ≈ 10 lines. Before the TextEdit runs: plain Enter is consumed and sends;
  Shift+Enter is left to the TextEdit (new line). Up/Down history only when the input has no
  newline.

### Header (US5)
- Remove heading and ⟳; theme switch moves to the right of the Changes / Info tabs.

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: substring + edit-distance only; no search index on disk; AI search reuses the haiku call pattern |
| II. Don't own Claude's data | PASS: reads session files and claude's stdout; the in-memory index is not persisted |
| III. Single binary | PASS: no new dependencies |
| IV. Learn by using | PASS: 10 s refresh, 40-session AI window and ranking weights are first guesses |

Post-design re-check: unchanged, PASS.

## Project Structure

```
src/
├── search.rs    NEW: index thread, scoring, ranking, AI prompt/parse (tested)
├── session.rs   Session::apply / push_prompt (parse uses apply)
├── runner.rs    streaming send (RunEvent), ask_haiku
└── app.rs       live card, search field + results, multi-line input, header removed
```
