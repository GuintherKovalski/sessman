# Feature Specification: Roadmap Rounds 1–4

**Feature Branch**: `012-roadmap-rounds-1-4` (built on `main`)

**Created**: 2026-10-15

**Status**: Implemented

**Input**: The 12 items of [ROADMAP.md](../../ROADMAP.md), written from the user's requests:
clickable URLs, file tab close button inside its tab, a prompt input that grows upwards and can
be resized/collapsed, a top bar clear of the macOS window buttons, a faster new workspace
(session), file tabs per session, workspaces that reopen their last session, the whole state
restored after a restart, Ctrl+C to stop Claude like in the terminal, a prompt queue while
Claude works, a Settings page, and Refine prompt with skippable clarifying questions.

The full description of each item (what, why, and edge cases) is in ROADMAP.md; this spec
lists what was accepted when it was built.

## User Scenarios & Testing *(mandatory)*

### US1 – Read and navigate (items 1, 2, 4) (P1)
1. **Given** a reply with `https://…`, `www.…` or `(https://…)`, **Then** each URL is a link
   (trailing punctuation outside), underlined on hover, opening in the browser; URLs in code
   and existing links are untouched; non-ASCII text never breaks it.
2. **Given** open file tabs, **Then** each tab's `×` is inside the tab, with its own hover;
   middle-click closes a tab.
3. **Given** the window (windowed or full screen), **Then** the macOS window buttons sit in a
   top bar above the panels; dragging the bar moves the window, double-click zooms.

### US2 – The prompt input (items 3, 9, 10, 12) (P1)
1. **Given** a long prompt, **Then** the input grows upwards to the max lines, then scrolls; a
   top-edge handle resizes it (double-click: automatic), a triangle collapses it to one line.
2. **Given** Claude is working, **Then** the input stays usable; Enter queues the prompt
   (listed above the input, `×` removes, click edits, Up pulls the last back); the next one is
   sent when the run ends.
3. **Given** Claude is working, **When** Ctrl+C (or ⏹), **Then** it stops at once; the partial
   reply stays once, marked ⏹ Interrupted; the prompt returns to the input; the queue pauses
   with Resume queue.
4. **Given** a prompt, **When** ✨ or Cmd+Enter, **Then** a helper model rewrites it, or first
   asks up to 3 questions with options, Other and Skip (Skip all); the result replaces the
   input and is never sent by itself; Undo refine / Cmd+Z restores; Esc cancels.

### US3 – Sessions, workspaces and restarts (items 5, 6, 7, 8) (P1)
1. **Given** + New session, **Then** the picker opens at once (in-process), at the default
   folder; after the first prompt the session is listed at once with ⏳.
2. **Given** files open in session A, **When** B is opened, **Then** B shows only its own tabs;
   back in A, its tabs, active tab, draft and filter come back.
3. **Given** a workspace tab is selected, **Then** its last session opens (else its newest;
   empty workspace: empty main area).
4. **Given** sessman is closed and reopened, **Then** window, panels, input size, tab, open
   session, every session's file tabs, drafts and (paused) queues, right tab and search come
   back; missing sessions/files are skipped; a broken state file is moved aside.

### US4 – Settings (item 11) (P2)
1. **Given** ⚙ or Cmd+,, **Then** Settings opens as a main-area tab with search and per-setting
   reset; changes apply and save at once (theme, zoom, mono size, send key, input height,
   drafts, tool lines, file view options, claude path / models / extra args / default folder,
   confirm delete, restore state, notifications, shortcuts list, about).

## Requirements
- Never write Claude's files; sessman's own files only (`$SESSMAN_CONFIG` overrides the folder).
- Tests for pure logic: linkify, input heights, tab close, view switching, state roundtrip and
  pruning, queue, stop (real process group), pending new sessions, interrupt marker, workspace
  last session, settings file and search, refine request/reply, send args, tab expansion,
  window builder.
