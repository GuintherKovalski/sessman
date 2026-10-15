# Research: Markdown Rendering, Styling, and Themes

## Markdown renderer
- **Decision**: `egui_commonmark` 0.25 (`default-features = false`, features `pulldown_cmark`, `better_syntax_highlighting`).
- **Rationale**: Built for egui 0.36 (our version); covers headings, emphasis, lists, quotes, links, task lists, syntect-highlighted code with light/dark themes. Without default features it doesn't load images (spec: images show as alt text).
- **Alternatives**: Hand-written renderer on `pulldown-cmark`: full control, but several hundred lines for lists/nesting/links. `egui_markdown`-style crates: less mature or not on egui 0.36.

## Tables
- **Decision**: Render top-level tables ourselves; leave the rest to `egui_commonmark`.
- **Rationale**: Its tables are a plain striped `Grid`: no column alignment, no bold header, no horizontal scroll (spec FR-002). Splitting the source at top-level table ranges (`pulldown_cmark::Parser::into_offset_iter`) is small and testable.
- **Alternatives**: Patch/fork the crate (maintenance), accept its tables (fails FR-002).

## Themes
- **Decision**: `egui::ThemePreference` + `ctx.set_theme`; eframe follows the macOS appearance for `System`. Persist in `~/.config/sessman/settings.json`.
- **Rationale**: Built in; no eframe `persistence` feature needed; consistent with the other config files.
- **Alternatives**: eframe persistence (adds a feature and stores egui memory we don't need).

## Long conversations
- **Decision**: Remember each message's rendered height; skip rendering messages outside the viewport.
- **Rationale**: Markdown widgets are heavier than one label per message; culling keeps per-frame work proportional to what's on screen.
