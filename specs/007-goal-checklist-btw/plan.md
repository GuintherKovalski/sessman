# Implementation Plan: Goal Checklist and /btw Side Questions

**Branch**: `007-goal-checklist-btw` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Research

- `claude -p --resume <id> --fork-session --no-session-persistence "<q>"` answers with the
  session's context, leaves the original file unchanged, and writes no new session (verified:
  the original kept its 29 lines and no file was added).
- Haiku follows a strict text format reliably; with prompts alone it marks everything open,
  so it gets a digest of both sides of the conversation and an explicit rule for [x].
- `claude -p` waits 3 s for stdin when it is not a terminal; commands now get a null stdin.

## Design

- `goal.rs`: parse `GOAL: …` + `- [x]` / `- [ ]` lines; `toggle` rewrites one marker. The raw
  text stays the stored format in `goals.json`, so older plain goals still parse.
- `session.rs`: `digest(messages, 400, 12000)` drops `[tool: …]` lines, cuts each message,
  keeps the first quarter and last three quarters when too long.
- `runner.rs`: shared `run()` helper; `summarize_goal(digest)` with the format prompt; `btw()`.
- `app.rs`: checklist with checkboxes; `/btw` prefix in `send`; answer box above the input.

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: /btw reuses the input, no new panel |
| II. Don't own Claude's data | PASS: fork is not persisted; goals live in sessman's config |
| III. Single binary | PASS |
| IV. Learn by using | PASS |
