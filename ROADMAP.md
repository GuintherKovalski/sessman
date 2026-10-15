# sessman: roadmap

Features for the next rounds. Each one is meant to go through Spec Kit like the others
(`/speckit-specify` with the text under its heading as the starting description). The rounds
are a suggested order: small UI fixes first, then the features that build on them.

## Round 1: UI polish

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

## Round 2: Interrupt and queue while Claude works

### 5. Ctrl+C stops Claude immediately, like in the terminal
- While a session is working (⏳), **Ctrl+C** stops it at once, as in Claude Code in the
  terminal: the running `claude -p` process for that session is killed right away, without
  waiting for the current step or tool to finish, and the ⏳ goes away in the same frame.
- It works whenever sessman's window has focus and the open session is working, whether or
  not the input is focused. On macOS there is no clash with copy (that is Cmd+C); on other
  systems Ctrl+C stops Claude only when no text is selected, otherwise it copies.
- A small **Stop** button next to the ⏳ does the same, for the mouse.
- What Claude had already written and done stays in the conversation, marked
  "Interrupted".
- The interrupted prompt is put back into the input so it can be fixed and sent again,
  if the input is empty; if something is already typed there, the prompt is left in the
  Up history instead. The next prompt continues the session from where it stopped
  (`--resume` picks up what was saved).
- Stopping does not send the queued prompts (item 6); the queue pauses, the same as after a
  failed run, so Ctrl+C means "stop", not "go to the next one".
- Only the session in view is stopped; other sessions keep running.
- Open question: the session file may end mid-step after a kill; check that `--resume`
  handles it, and what the conversation shows.

### 6. Input stays usable while Claude works; Enter queues the prompt
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
  queue (or none). Queues are kept in memory only; quitting sessman drops them (with a
  warning if any are waiting).
- If a run fails, the queue pauses (not sent) and shows why, with a **Resume queue**
  action.

## Round 3: Settings

### 7. Settings page
- A **⚙ Settings** button (bottom of the left panel, next to search, and **Cmd+,** on macOS
  / **Ctrl+,** elsewhere) opens the settings in the main area, as a tab like the file tabs.
- Settings are grouped into sections with a search field on top, and saved immediately to
  `~/.config/sessman/settings.json` (which already holds the theme). A **Reset to default**
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
  - **Behaviour**: confirm before deleting a workspace; restore the last open session and
    file tabs at start; notify (system notification) when a run finishes while sessman is
    in the background.
  - **Keyboard shortcuts**: a read-only list of all shortcuts for now (rebinding later).
  - **About**: version, config folder (with **Open in Finder**), status-line integration
    state.

## Round 4: Refine prompt

### 8. Refine prompt before sending
- A **Refine** button at the right edge of the prompt input, and a shortcut
  (**Cmd+Enter** on macOS, **Ctrl+Enter** elsewhere; if the "send with Cmd/Ctrl+Enter"
  setting from item 7 is on, use **Cmd/Ctrl+Shift+Enter** instead).
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
