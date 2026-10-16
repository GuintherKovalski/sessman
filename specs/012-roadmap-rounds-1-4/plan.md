# Implementation Plan: Roadmap Rounds 1–4

## Approach

One pass over `app.rs` with the logic pulled into small testable modules:

| Item | Where | How |
|------|-------|-----|
| 1 Links | `markdown::linkify` | Walk pulldown-cmark text events (outside links/code), wrap bare URLs as `<url>` / `[www…](https://www…)` before egui_commonmark renders; also for table cells. |
| 2 Tab × | `app::closable_tab` | One allocated rect painted as the tab, with an inner `×` rect that has its own hover; middle-click closes. |
| 3 Input | `app::prompt_bar`, `input_heights` | The bottom panel only grows if asked, so the text height is measured each frame (`content_size`) and requested next frame within min..max; drag handle + triangle; height/collapse saved in state. |
| 4 Top bar | `main::viewport`, `app::title_bar` | `fullsize_content_view`, transparent title bar; a 28 px top panel that starts a window drag / toggles zoom. |
| 5 New session | `runner::pick_folder`, `pending_new` | `NSOpenPanel` in-process (objc2-app-kit, already in the tree) instead of `osascript` (≈2 s to activate); pending rows from running new runs; poke the indexer on the first streamed line. |
| 6 Per-session tabs | `state::Views` | Swap the open session's view (files, active, draft, filter) in/out when the main area changes session. |
| 7 Workspace last | `Workspaces::{last,set_last}`, `pick_session` | Saved in workspaces.json. |
| 8 Restore | `state::State`, eframe `persistence` | `state.json` written every 2 s when changed and on quit (atomic rename); eframe saves window + egui memory in `window.ron`. |
| 9 Stop | `runner::Stop`, `spawn_stream` | Spawn in its own process group; SIGINT the group, SIGKILL after 2 s; Claude's saved interrupt marker shown as ⏹ Interrupted. |
| 10 Queue | `queue::Queue` | Per session; next on normal end; pause on stop/failure/restart. |
| 11 Settings | `settings.rs` | Typed `Settings` (serde defaults keep old files loading); page with search and reset; `runner::configure` for claude path/models/args. |
| 12 Refine | `refine.rs` | JSON request/reply with the helper model; questions UI above the input. |

## Decisions
- Settings deferred: code theme, code-block wrapping, timestamps (need more than a setting).
- Esc for refine is handled globally (egui drops focus on Esc before widgets see it).
- Clicking another text field in the main area keeps its focus (the click-anywhere rule skips it).
