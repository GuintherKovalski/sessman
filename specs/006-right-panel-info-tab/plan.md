# Implementation Plan: Right Panel Tabs with Session Info

**Branch**: `006-right-panel-info-tab` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Research

- Session files record `message.model` and `message.usage` per reply, but not the context
  window size or usage limits.
- Per https://code.claude.com/docs/en/statusline.md, the status-line command gets JSON on
  stdin with `context_window.context_window_size`, `context_window.used_percentage`, and
  (Pro/Max only, after the first reply) `rate_limits.{five_hour,seven_day}.{used_percentage,resets_at}`.
  There is no other documented way to read limits.
- Per https://code.claude.com/docs/en/model-config.md, current models (Opus 4.7+, Sonnet 5+,
  Haiku 5.5, Fable) default to a 1M window; older ones are 200k unless `[1m]`.
- `claude -p --no-session-persistence` runs without saving a session.

## Design

- `session.rs`: track latest `model` and context tokens (input + cache read + cache creation,
  as Claude Code counts it).
- `status.rs`: `sessman statusline` subcommand saves the stdin JSON to
  `~/.config/sessman/status/<session_id>.json` (and `latest.json` when it has limits) and
  prints "Opus · ctx 18% · 5h 23% · 7d 41%". `enable()` sets `statusLine` in
  `~/.claude/settings.json` (keys kept in order via serde_json `preserve_order`), only from a button.
- `runner.rs`: `summarize_goal` runs `claude -p --model haiku --no-session-persistence`.
  Goals are stored in `~/.config/sessman/goals.json`.
- `app.rs`: right panel tab switch; Info tab sections; limits re-read every 5 s.

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: summaries on demand, no polling of Anthropic APIs |
| II. Don't own Claude's data | PASS with note: `settings.json` is changed only when the user clicks Enable, and only `statusLine` |
| III. Single binary | PASS: the status-line command is the same binary |
| IV. Learn by using | PASS |
