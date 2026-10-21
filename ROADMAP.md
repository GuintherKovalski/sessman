# scope: roadmap

Features for the next rounds. Each one is meant to go through Spec Kit like the others
(`/speckit-specify` with the text under its heading as the starting description). The rounds
are a suggested order: small UI fixes first, then the features that build on them.

Items 1–12 are implemented (`specs/012-roadmap-rounds-1-4/`); PROJECT.md describes how they
work now. Item 13 is planned.

## Overview

| # | Round | Feature | In short | Depends on | Status |
|---|-------|---------|----------|------------|--------|
| 1 | 1 | [Clickable URLs in replies](#1-clickable-urls-in-replies) | Bare URLs highlighted and open in the browser | | Done |
| 2 | 1 | [File tab close button](#2-file-tab-close-button-attached-to-its-tab) | The `x` is part of its tab | | Done |
| 3 | 1 | [Prompt input grows up, resizable](#3-prompt-input-grows-upwards-and-can-be-resized) | Grows upwards; drag to resize or collapse | | Done |
| 4 | 1 | [Top bar for macOS window buttons](#4-top-bar-clear-of-the-macos-window-buttons) | Empty bar so close / minimize don't cover the UI | | Done |
| 5 | 1 | [Faster new workspace](#5-opening-a-new-workspace-is-faster) | Folder picker and new session ready at once | | Done |
| 6 | 1 | [File tabs per session](#6-file-tabs-belong-to-their-session) | Each session shows only its own file tabs | | Done |
| 7 | 1 | [Workspace opens its last session](#7-selecting-a-workspace-opens-its-last-session) | Selecting a workspace switches to its last session | 6 | Done |
| 8 | 1 | [Save and restore state](#8-the-whole-state-is-saved-and-restored-on-restart) | Everything comes back after a restart | 3, 6, 7 | Done |
| 9 | 2 | [Ctrl+C stops Claude](#9-ctrlc-stops-claude-immediately-like-in-the-terminal) | Kills the run at once, prompt back to the input | | Done |
| 10 | 2 | [Prompt queue](#10-input-stays-usable-while-claude-works-enter-queues-the-prompt) | Input never blocked; Enter queues while Claude works | 9 | Done |
| 11 | 3 | [Settings page](#11-settings-page) | IDE-style settings in the main area | 8 | Done (3 settings deferred, see item) |
| 12 | 4 | [Refine prompt](#12-refine-prompt-before-sending) | Model rewrites the prompt, asks skippable questions | 11 | Done |
| 13 | 5 | [Live changes](#13-live-changes-scope-updates-without-ending-its-sessions) | Rebuild and reload scope without ending the sessions it runs | 8 | Planned |

## Round 1: UI polish and navigation

### 1. Clickable URLs in replies
- Every URL in Claude's replies is highlighted like a hyperlink (link color, underline on
  hover, pointer cursor) and opens in the default browser when clicked.
- Today only Markdown links (`[text](url)`) are clickable. Bare URLs (`https://…`,
  `www.…`) written as plain text should behave the same, including inside lists, quotes and
  tables. URLs inside code blocks and inline code stay plain text.
- Trailing punctuation (`.`, `,`, `)`, `:`) is not part of the link.

### 2. File tab close button attached to its tab
- When files are open in the middle area, the `x` that closes a file tab sits inside the
  tab itself (same background, border and highlight as the tab), not detached next to it.
- The `x` is dimmed until the tab or the `x` is hovered; hovering the `x` gives it its own
  hover background so it is clear what will be clicked.
- Middle-click on a tab also closes it (as in browsers and IDEs).

### 3. Prompt input grows upwards and can be resized
- As text is added, the input grows **upwards** (its bottom edge stays put, above the
  window's bottom) and the conversation above it shrinks, instead of the input pushing
  content around.
- The top edge of the input has a drag handle, like the left and right panels already have,
  so it can be made taller or shorter by dragging vertically. Double-clicking the handle
  resets it to the automatic size.
- It can be collapsed to a single line and expanded again (same behaviour as the side
  panels). The chosen height is remembered between runs.
- Up / Down history and Shift+Enter keep working as now.

### 4. Top bar clear of the macOS window buttons
- A thin horizontal bar across the top of the window, above the workspace tabs and the
  Changes / Info tabs, so nothing sits where the macOS close / minimize / zoom buttons
  appear. Today, when those buttons slide down (e.g. hovering the top edge in full screen),
  they cover the elements at the top of the panels.
- The bar is empty for now (same color as the panels, a subtle bottom border) and dragging
  it moves the window, like a title bar. Double-clicking it zooms the window, as macOS does.
- Its height matches the macOS title bar area (~28 px), so the window buttons fit inside it
  in both windowed and full-screen mode. On other systems it can be thinner.
- Later it can hold things like the session name or a global search.

### 5. Opening a new workspace is faster
- Starting work in a new folder (**+ New session** → pick a folder → first prompt) takes
  noticeably long before scope is ready. It should feel instant: the folder picker opens
  right away, and the new session is usable as soon as the folder is chosen.
- Measure first where the time goes. Likely causes, from the current code:
  - The folder picker runs through `osascript` (`runner::pick_folder`), which starts a new
    AppleScript process each time; a native picker (e.g. the `rfd` crate) opens faster.
  - After the first prompt, the session only appears in the list once `claude` has written
    its file and the next list refresh picks it up; show it in the list (with ⏳) as soon as
    the prompt is sent.
  - Any full rescan of `~/.claude/projects` or search-index rebuild triggered by the new
    folder should not block the UI.
- If "workspace" here means switching workspace tabs, the same applies: check for slow
  reloads on tab change.

### 6. File tabs belong to their session
- Every open file tab belongs to the session it was opened from. When another session is
  selected, only that session's file tabs are shown; the other session's tabs disappear
  from the tab bar.
- Switching back to a session brings back its file tabs as they were: the same files, the
  same active tab (or Main), scroll position and Rendered / Source choice.
- Today the open file tabs are one list for the whole app (`file_tabs` in `src/app.rs`), so
  files from one session stay visible in every other session.
- A new, not yet created session starts with no file tabs. Closing a file tab only closes it
  in the current session. Open tabs are saved and restored on restart (item 8).

### 7. Selecting a workspace opens its last session
- Clicking a workspace tab (or **All**) switches the main area to the session that was last
  open in that workspace, instead of leaving the previous workspace's session on screen.
- scope remembers, per workspace, the last session opened there. If there is none yet (or
  it was moved out or deleted), the most recently active session in the workspace opens.
  An empty workspace shows the empty main area.
- Coming back to a workspace restores that session as it was, with its file tabs (item 6),
  and puts the cursor in the prompt input.
- The remembered session per workspace is saved with the workspaces, so it survives a
  restart.

### 8. The whole state is saved and restored on restart
- Closing scope and opening it again brings everything back as it was, with no action
  needed: saving happens automatically while the app is used (shortly after each change,
  and on quit), and loading happens at start.
- What is restored:
  - window size and position (and full screen), widths of the left and right panels,
    height of the prompt input (item 3) and which panels are collapsed;
  - the selected workspace tab and the open session, and per workspace its last session
    (item 7);
  - per session: open file tabs, the active tab, scroll positions, Rendered / Source choice
    (item 6), the text being typed in the prompt input (draft) and the queued prompts
    (item 10);
  - the right panel tab (Changes / Info) and its file filter;
  - the session search text.
- Saved in scope's own config folder (e.g. `~/.config/scope/state.json`), never in
  Claude's files. Things that no longer exist (deleted sessions, missing files, removed
  workspaces) are skipped quietly.
- Prompts that were running when scope closed can't continue (the `claude` process ends
  with the app); their sessions open showing what was saved, and their queues come back
  paused, with **Resume queue**, so nothing is sent without the user asking.
- A broken or old-format state file never stops the app from starting: it starts fresh and
  keeps the bad file aside.

## Round 2: Interrupt and queue while Claude works

### 9. Ctrl+C stops Claude immediately, like in the terminal
- While a session is working (⏳), **Ctrl+C** stops it at once, as in Claude Code in the
  terminal: the running `claude -p` process for that session is killed right away, without
  waiting for the current step or tool to finish, and the ⏳ goes away in the same frame.
- It works whenever scope's window has focus and the open session is working, whether or
  not the input is focused. On macOS there is no clash with copy (that is Cmd+C); on other
  systems Ctrl+C stops Claude only when no text is selected, otherwise it copies.
- A small **Stop** button next to the ⏳ does the same, for the mouse.
- What Claude had already written and done stays in the conversation, marked
  "Interrupted".
- The interrupted prompt is put back into the input so it can be fixed and sent again,
  if the input is empty; if something is already typed there, the prompt is left in the
  Up history instead. The next prompt continues the session from where it stopped
  (`--resume` picks up what was saved).
- Stopping does not send the queued prompts (item 10); the queue pauses, the same as after a
  failed run, so Ctrl+C means "stop", not "go to the next one".
- Only the session in view is stopped; other sessions keep running.
- Open question: the session file may end mid-step after a kill; check that `--resume`
  handles it, and what the conversation shows.

### 10. Input stays usable while Claude works; Enter queues the prompt
- The prompt input is never blocked while a session is working (today it is disabled with
  "claude is working on this session…"). It can be typed in, edited, and use Up / Down
  history as usual.
- Pressing **Enter** while the session is working adds the prompt to that session's
  **queue** instead of sending it, and clears the input.
- Queued prompts are shown right above the prompt input, one compact line each (dimmed,
  with a "queued" marker and their order), newest at the bottom, nearest the input. Long
  prompts are cut to one or two lines; clicking one expands it.
- Each queued prompt has an `x` to remove it, and can be clicked to edit it (it moves back
  into the input). **Up** in an empty input also pulls the last queued prompt back for
  editing, as Claude Code does.
- When the current run finishes normally, the first queued prompt is sent automatically,
  then the next when that one finishes, and so on. It appears in the conversation as a
  normal "You" message at that point.
- The queue belongs to the session: switching to another session shows that session's own
  queue (or none). Queues are saved and restored on restart, paused (item 8).
- If a run fails, the queue pauses (not sent) and shows why, with a **Resume queue**
  action.

## Round 3: Settings

### 11. Settings page
- A **⚙ Settings** button (bottom of the left panel, next to search, and **Cmd+,** on macOS
  / **Ctrl+,** elsewhere) opens the settings in the main area, as a tab like the file tabs.
- Settings are grouped into sections with a search field on top, and saved immediately to
  `~/.config/scope/settings.json` (which already holds the theme). A **Reset to default**
  link per setting.
- First set (the basics most IDEs have; more will be added later):
  - **Appearance**: theme (dark / light / system, moves here from the Info tab buttons),
    UI zoom / font size, monospace font size, code highlighting theme.
  - **Prompt input**: send with Enter or with Cmd/Ctrl+Enter; maximum height before it
    scrolls; keep a draft per session when switching sessions.
  - **Conversation**: show tool lines (`⚙ Read · Edit`) or hide them; wrap long code
    lines or scroll sideways; show timestamps on messages.
  - **Files**: show line numbers; wrap lines; tab width; open `.md` files rendered or as
    source.
  - **Claude**: path to the `claude` binary; model for prompts (default = Claude Code's
    default); model for the helpers (search, goals, refine prompt; default Haiku); extra
    arguments for `claude -p` (e.g. permission mode, allowed tools); default folder for new
    sessions.
  - **Behaviour**: confirm before deleting a workspace; restore the previous state at start
    (item 8; on by default); notify (system notification) when a run
    finishes while scope is in the background.
  - **Keyboard shortcuts**: a read-only list of all shortcuts for now (rebinding later).
  - **About**: version, config folder (with **Open in Finder**), status-line integration
    state.
- Deferred (not built yet): code highlighting theme, wrapping long lines in conversation code
  blocks, and message timestamps. Each needs more than a setting (the session parser keeps no
  timestamps; code blocks and their theme are drawn by egui_commonmark).

## Round 4: Refine prompt

### 12. Refine prompt before sending
- A **Refine** button at the right edge of the prompt input, and a shortcut
  (**Cmd+Enter** on macOS, **Ctrl+Enter** elsewhere; if the "send with Cmd/Ctrl+Enter"
  setting from item 11 is on, use **Cmd/Ctrl+Shift+Enter** instead).
- It takes the text in the input and asks a model (the helper model from Settings, Haiku by
  default, via `claude -p --no-session-persistence`, with the session's goal and recent
  prompts as context) to rewrite it as a clear, well-organized prompt: same intent, nothing
  added that the user did not ask for, structured into steps or bullet points when it has
  several parts.
- If the prompt is ambiguous, the model returns clarifying questions instead of (or
  before) the rewrite. They are shown above the input, each with a few clickable options
  and a free-text "Other". Every question can be skipped, and **Skip all** refines with what
  is known. The answers are folded into the refined prompt.
- The refined prompt replaces the input text for review and editing; it is never sent
  automatically. **Cmd/Ctrl+Z** (or an **Undo refine** link) restores the original text.
- While refining, the input shows a spinner and stays editable; Esc cancels.

## Round 5: Developing scope from inside scope

### 13. Live changes: scope updates without ending its sessions
- **Why**: scope is developed with Claude sessions that scope itself runs. Each run is a
  child of the app (`claude -p` with its output piped to it), so restarting scope to see a
  change ends every running session, including the one making the change. For now changes are
  tested in a separate instance (`scripts/test-instance.sh`, see CLAUDE.md), and the developer
  restarts their own scope by hand to take them.
- **Runs outlive the app** (done). It turned out they already did: `claude -p` keeps going
  and saves its whole reply when the app reading its output quits; scope only lost track of
  them. Now scope finds every `claude -p` on a listed session that isn't its own child (`ps`
  every 2 s) and follows it through the session file until its process ends, with Stop and the
  queue working on it (PROJECT.md, "Runs scope did not start"). What a restart still loses:
  the letter-by-letter text of a reply in progress (it arrives a step at a time instead), and
  restored queues come back paused. If that matters, the next step is the `runs/<id>.jsonl`
  output file: runs write their stream-json there instead of a pipe, and a restarted scope
  tails it.
- **Restart in place.** With runs detached and the state already restored on start (item 8),
  a restart loses nothing. scope watches its own binary (`std::env::current_exe()`): when a
  new build or bundle replaces it, a quiet **Restart to update** button appears in the top bar
  (and `scope restart` does the same from a session). It saves the state and `exec`s the new
  binary, so the window comes back in the same place, on the same session, with its runs still
  streaming.
- **Agents' loop**: a session changes the code, tests it in a separate instance, then runs
  `scripts/bundle.sh` and `scope restart` itself; it keeps running through the restart and
  sees its change live in the IDE it runs in.
- Not planned: hot-reloading UI code into the running process (dylib reloading). It is fragile
  with egui's state and saves only the second or two a restart takes.
- Until restart-in-place is done, CLAUDE.md's rule stands: every test runs in a new instance
  of the IDE, never the running one.
