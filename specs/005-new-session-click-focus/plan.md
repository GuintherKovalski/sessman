# Implementation Plan: New Session Button and Click-to-Focus Input

**Branch**: `005-new-session-click-focus` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Summary

- Click-to-focus: after drawing the central panel, if `pointer.primary_clicked()` happened
  inside it, set `focus_prompt`; the prompt bar calls `request_focus()` when the flag is set.
  Also set when a session is opened or a new one started.
- New session: the button runs `osascript … choose folder` on a background thread (no new
  dependency; the native macOS picker). On a folder, the app enters a draft state
  (`new_session: Some(folder)`, an empty `Session` with that `cwd`). Sending runs
  `claude -p --session-id <fresh uuid v4> <prompt>` in the folder (runner gains a `new` flag).
  When it completes, the list is reloaded and the session with that id is selected. The uuid
  comes from 16 bytes of `/dev/urandom`.

## Constitution Check

| Principle | Status |
|-----------|--------|
| I. Minimal First | PASS: native picker via osascript, no form |
| II. Don't own Claude's data | PASS: Claude creates the session; we only choose its id |
| III. Single binary | PASS: no new dependency |
| IV. Learn by using | PASS |
