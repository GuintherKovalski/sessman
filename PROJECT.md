# scope: project overview

scope is a small desktop app, written in Rust, for browsing, organizing and continuing
your Claude Code sessions. It reads the sessions Claude Code already keeps on disk; it does
not replace Claude Code.

It is deliberately minimal: the goal is to use it, find out what feels right, and grow it from
there. Each change is specified first with Spec Kit (see [How the project is built](#how-the-project-is-built)).

## The window

A thin bar runs along the top of the window, above everything else. The macOS close /
minimize / zoom buttons sit in it (the content runs under a transparent title bar), so they
never cover the panels, also in full screen. Drag the bar to move the window; double-click
it to zoom.

```
┌──────────────────┬──────────────────────────────────────┬─────────────────────┐
│ All │ A │ B │ +  │ ┌──────────────────────────────────┐ │ Changes │ Info  ☾☀▣ │
│                  │ │ You  (pinned prompt)             │ │ src/app.rs          │
│ [+ New session ] │ └──────────────────────────────────┘ │ - old line          │
│▌Session title    │ You                                  │ + new line          │
│▌project folder   │ ...                                  │                     │
│ Session title    │ Claude                               │ src/main.rs         │
│ ...              │ ...                                  │ + added line        │
│                  │ ┌──────────────────────────────────┐ │                     │
│                  │ │ > type a prompt_                 │ │ ...                 │
│ 🔍 Search        │ └──────────────────────────────────┘ │                     │
└──────────────────┴──────────────────────────────────────┴─────────────────────┘
```

### Left: sessions and workspaces
- **+ New session** (below the tabs, above the list): pick the folder Claude should work in (the
  native macOS folder picker, which opens at once, starting at the default folder from Settings
  or else the current session's folder). The main area shows an empty new session; your first
  prompt creates it. It is listed at once (with ⏳) and becomes a normal, selected session as
  soon as Claude has written its file. If a workspace tab is open, the new session joins that
  workspace.
- Lists every Claude Code session, newest first. Each row shows the session title (Claude's
  generated title, or your first prompt) and its project folder. The list refreshes on its
  own (every 10 s and after each reply), so sessions started elsewhere show up by themselves.
  A session that is working shows ⏳, and `+N queued` when prompts are waiting for it.
- **⚙** (next to the search, or **Cmd+,**) opens the Settings page in the main area.
- **Search** (bottom of the panel): typing filters the list (within the open tab). Name
  matches come first, then goal summaries, then the conversations' own text (with a snippet
  under the name). Close spellings still match names and goals. **Enter** asks Claude Haiku
  (a little of your usage) which of the 40 most recent sessions match what you described,
  and lists its picks with a reason each. Editing the query goes back to live results;
  **Escape** clears it.
- Click a session to open it in the middle.
- **Double-click** a session name to rename it, like a folder: the name is selected, type the
  new one, **Enter** (or click elsewhere) saves, **Escape** cancels. Saving an empty name
  brings back the original one. Names are scope's own; Claude Code keeps its own title.
- **Tabs** at the top: "All sessions", one tab per workspace, and **+**. Selecting a workspace tab
  opens the session that was last open in it (or its newest session; an empty workspace shows
  an empty main area); each remembers its last session across restarts. "All sessions" opens
  on the workspaces overview.
  - **+** asks for a name and creates a workspace tab.
  - Drag a session onto a workspace tab to put it there (the tab is outlined while you hover).
  - Or right-click a session → **Move to** → pick a workspace.
  - Inside a workspace tab, right-click a session → **Remove from workspace**.
  - Right-click a workspace tab → **Delete workspace** (asks first, unless turned off in
    Settings; the sessions themselves are untouched).
  - A session belongs to at most one workspace; "All sessions" always shows everything.
  - Every workspace has its own color (no two share one). Its tab is tinted with it, and in
    "All sessions" each session in a workspace gets a colored stripe and tint (the ▌ above).

### Middle: workspaces overview (All sessions)
In "All sessions", the main area shows an overview of the workspaces until a session is
opened; clicking "All sessions" again comes back to it.
- One card per workspace, in its tab's color (a dot by the name, a stripe down the left
  edge, a light tint). Its header shows how many sessions it has, how recent the latest
  activity is, a spinner while Claude works in one, and queued prompts. Click the name to
  open the workspace.
- Under it: the folder and git branch, then what the helper model made of its 8 newest
  sessions: **Purpose** (why the work exists), **Now** (what is being done most recently)
  and **Heading** (the end state it works towards), plus up to 3 Jira tickets, epics and
  parent tickets first. Only keys that really appear in the sessions or the branch are kept;
  with a Jira URL in Settings they link to the ticket.
- Summaries are kept in `~/.config/scope/overview.json` and are written when missing, or
  again when there was activity since and they are at least 30 min old (up to 3 at a time).
  **↻** on a card, or **↻ Refresh all**, writes them again now.

### Workspace orchestration
- In a workspace tab, the first bubble in the list is the workspace itself: **Orchestration**,
  drawn unlike the session bubbles (filled with the workspace color, a thicker outline, a ◆ mark,
  a line like `6 sessions · 2 working · 62% · 1 needs you`, and the progress along its bottom).
- Clicking it opens the orchestration page in the main area (the tab reopens on it; an empty
  workspace opens on it too): overall **progress**, a summary, **Needs you**, **Next**, then a
  card per session with its live state (working spinner and tool, queued prompts, last activity,
  goal checklist) and what the report says (working / waiting / blocked / done / idle, what it is
  doing, its next step). Click a card to open the session.
- The report is written by the orchestration model (the helper model unless set) from the
  orchestration harness in Settings. Updates are incremental: the previous report plus only the
  sessions with new activity. It updates on its own when missing, or every N minutes (Settings)
  while there is news and the page is open; **↻ Update** asks now.
- **Clear & re-summarize** forgets the report, what it has seen and the questions, and
  summarizes every session again from scratch.
- The field at the bottom asks the orchestrator about the workspace (report, recent questions
  and a short digest of each session); answers stack on the page.
- Wherever the orchestrator mentions a session (summary, Needs you, Next, the cards and its
  answers), it reads as the session's current name and is a link: click it to open the session.
- Saved in `~/.config/scope/orchestration.json`, by workspace name.

### Middle: conversation and file tabs
- When a file is open, a tab bar appears on top: **Main** (the conversation) and one tab per
  open file, with the active one highlighted. Each tab has its `×` inside it (middle-click
  closes too). Files are **read-only**, with line numbers and syntax highlighting; text can be
  selected and copied. They reload whenever the session reloads, so they show Claude's latest
  changes. Markdown files (`.md`) open **rendered**; **Rendered / Source** at the top switches
  to the raw text.
- **File tabs belong to their session.** Opening another session shows only its own tabs;
  coming back brings yours back as they were (active tab, scroll position, Rendered / Source,
  the Changes filter, and what you were typing).
- **Click anywhere** in the main area to put the cursor in the prompt input, like a terminal.
  Opening a session also focuses it.
- **Prompt input** at the bottom, styled like a terminal line (`>` prompt, monospace). With it
  focused:
  - It grows **upwards** as you type, up to 10 lines (a setting), then scrolls; the
    conversation above shrinks. **Shift+Enter** starts a new line.
  - The short bar on its top edge is a drag handle, like the side panels': drag it to make the
    input taller or shorter; double-click it for the automatic size. The ▾ at its right
    collapses the input to one line (▴ expands it). The size is kept across restarts.
  - **Up / Down** step through the prompts you sent earlier in this session (like shell
    history) while the input is a single line. Going past the newest brings back what you
    were typing.
  - **Enter** sends the prompt to the session. It shows in the conversation at once, and
    Claude's progress follows live: the reply as it is written, a `⚙ Tool…` line when a tool
    starts, and each finished step (and its file changes, in the Changes tab) before the
    whole run ends.
  - **While Claude works, the input stays usable** and Enter **queues** the prompt. Queued
    prompts are listed right above the input; each is sent when the run before it ends. `x`
    removes one, clicking one moves it back into the input to edit, and **Up** in an empty
    input pulls back the newest. Queues belong to their session.
  - **Ctrl+C** (or the ⏹ button next to the spinner) stops Claude at once, like in the
    terminal: claude and everything it started get SIGINT (SIGKILL 2 s later if still there).
    What it had written stays, marked **⏹ Interrupted**; the stopped prompt comes back into
    the input to fix and resend; the queue **pauses** (with **Resume queue**) so nothing else
    is sent unasked. A failed run pauses the queue the same way.
  - **✨ Refine** (the button at the input's right, or **Cmd+Enter**) asks the helper model
    (Haiku by default) to rewrite the prompt clearly, using the session's goal and recent
    prompts as context. If the prompt is ambiguous it first asks up to 3 questions above the
    input, each with suggested answers, an "Other…" field and **Skip**; **Skip all** refines
    with what is known. The refined text replaces the input for review; it is never sent by
    itself. **Undo refine** (or **Cmd+Z**) brings back your text; **Esc** cancels a refine
    in progress.
  - Several sessions can run at once: while one works (⏳ next to its name in the list), you
    can open another, or start a new one, and send it a prompt. Switching back to a working
    session shows what it has done so far and keeps streaming.
- **Pinned "You"**: the prompt that the reply at the top of your view answers stays pinned
  above the conversation, so you always know what a long reply is about.
- The conversation itself: one card per message (blue accent for you, orange for Claude),
  rendered as Markdown: headings, lists, bold/italic, links, including bare URLs like
  `https://…` and `www.…` (highlighted, underlined on hover, click to open in the browser),
  quotes, code blocks with syntax highlighting, and **tables** as real tables (bold header,
  aligned columns, scroll sideways when wide). Tool calls are a small dimmed line
  (`⚙ Read · Edit`). Only messages near the visible area are drawn, so long sessions scroll
  smoothly.

### Right: Changes and Info tabs
- **Changes**: one card per changed file, in order of its first change: file name, folder,
  and `+added -removed` counts, then each edit as diff hunks with syntax highlighting, green /
  red tinted rows for added / removed lines, and unchanged stretches collapsed to 3 lines of
  context (`⋮` marks a gap). At the bottom, **Files** lists every changed file:
  - **click** a file (or a card's header) to show only its changes; click again or
    **Show all** to see everything;
  - **double-click** to open it in the main area (see below).
  Only edits made with Claude's Edit / MultiEdit / Write tools are known; changes made through
  shell commands don't show up here.
- **Info** (top to bottom):
  - **Goal**: until you click **Summarize goal** it shows the start of the first prompt. The
    button asks Claude Haiku (a little of your usage) for the goal plus a **checklist of 3–7
    subgoals**, ticked where the conversation shows they're done, with "N of M done". You can
    tick and untick items yourself; that is saved.
  - **Model** of the latest reply.
  - **Context**: tokens in context vs the window size (e.g. 204k / 1.0M).
  - **Usage limits**: your 5-hour and weekly limits with time until reset. Claude Code only
    shares these with its status line, so click **Enable** once: it sets scope as Claude
    Code's status line (`statusLine` in `~/.claude/settings.json`). Limits then appear after
    your next reply in an interactive Claude Code session (Pro/Max plans). If you already
    have a status line, scope leaves it alone.
  - **Ask about this goal**: the last item, pinned to the bottom of the panel. Enter sends the question, together
    with the goal and checklist, to the session's own model with its full context, like
    Claude Code's `/btw`: nothing is added to the session. The answer shows just above the field
    until you click Dismiss.

### Settings
**⚙ Settings** opens as a tab in the main area (close it with its `×`). A search field filters
the settings; each changed setting has a ↺ to reset it. Changes apply and save at once.
- **Appearance**: theme (dark / light / system), zoom, monospace font size.
- **Prompt input**: send with Cmd/Ctrl+Enter instead of Enter (Refine then moves to
  Cmd+Shift+Enter), maximum height in lines, keep a draft per session.
- **Conversation**: show or hide the `⚙ Read · Edit` tool lines.
- **Files**: line numbers, wrap long lines, tab width, open Markdown rendered.
- **Claude**: path to `claude`, model for prompts, helper model (search, goals, refine),
  extra arguments for every `claude -p` (e.g. `--permission-mode acceptEdits`), default
  folder for new sessions.
- **Overview**: Jira URL the ticket keys link to (e.g. `https://acme.atlassian.net`).
- **Harness**: the instructions each helper starts from, editable (empty = default, ↺ resets):
  workspace orchestration (plus its model and update interval), questions in the right panel,
  and finding sessions in the left panel. The reply formats the app parses stay fixed.
- **Behaviour**: confirm before deleting a workspace, restore the previous state at start,
  notify when a run finishes while scope is in the background.
- **Keyboard shortcuts** (a list) and **About** (version, config folder with Open in Finder,
  status line).

### Restarting
Closing and opening scope brings everything back: window size and position, panel widths,
the input's size, the workspace tab and open session, every session's file tabs, scroll
positions, drafts and queued prompts (queues come back paused), the right panel tab and the
search text. Sessions or files that no longer exist are skipped; a damaged state file is
moved aside (`state.json.bad`) and the app starts fresh.

## How it works

| What | Where it comes from |
|------|---------------------|
| Session list and conversations | `~/.claude/projects/<project>/<session-id>.jsonl`, written by Claude Code. Read-only. |
| Session title | The `ai-title` entry in the session file, else the first prompt. |
| Prompt history | The prompts you typed in that session (tool results and slash-command wrappers are skipped). |
| Diffs | The `Edit` / `MultiEdit` / `Write` tool calls recorded in the session, diffed line by line. |
| Sending a prompt | Runs `claude -p [extra args] --resume <session-id> [--model <m>] --output-format stream-json --verbose --include-partial-messages "<prompt>"` in the session's folder, in its own process group, in the background; its streamed lines update the open conversation as they arrive. |
| Stopping (Ctrl+C) | SIGINT to that process group (SIGKILL after 2 s). Claude Code saves the partial reply and an `[Request interrupted by user]` marker, shown as ⏹ Interrupted. |
| Queue | Kept per session in memory and in `state.json`; the next prompt is sent when a run ends normally. |
| Workspaces overview | `claude -p --model <helper> --no-session-persistence` with a digest of the workspace's 8 newest sessions (title, goal, conversation), the git branch and the Jira keys found in them; it answers in JSON with purpose, now, heading and tickets. |
| Refine | `claude -p --model <helper> --no-session-persistence` with the prompt, the goal and the last 3 prompts; it answers in JSON with either the refined prompt or up to 3 questions. |
| New session | Runs `claude -p --session-id <new uuid> "<first prompt>"` in the chosen folder. |
| Model, context tokens | The latest reply's `model` and `usage` in the session file. |
| Context size, usage limits | Claude Code's status-line data, saved by `scope statusline` to `~/.config/scope/status/`. Without it, the size is inferred from the model. |
| Search | An in-memory index (names, goals, conversation text) rebuilt in the background for changed files every 10 s. Enter-search: `claude -p --model haiku --no-session-persistence` with a numbered list of recent sessions (name, goal, first prompts). |
| Goal and checklist | `claude -p --model haiku --no-session-persistence` with a digest of the conversation, on request; saved (with your ticks) to `~/.config/scope/goals.json`. |
| Goal questions | `claude -p --resume <id> --fork-session --no-session-persistence "<question>"`: full context, original session untouched, nothing saved. |
| Settings (theme included) | scope's own file: `~/.config/scope/settings.json`. |
| App state | `~/.config/scope/state.json` (sessions' tabs, drafts, queues, layout) and `window.ron` (window, panel sizes, scroll positions, saved by eframe). Saved every 2 s when changed and on quit. |
| Bare URLs | Turned into Markdown autolinks before rendering (not inside code or existing links). |
| Workspaces and their colors | scope's own file: `~/.config/scope/workspaces.json`. |
| Custom session names | scope's own file: `~/.config/scope/names.json`. |

scope never writes to Claude's session files. Everything it adds (workspaces, names, goals,
settings, app state, status data) lives in its own config folder (`$SCOPE_CONFIG` overrides
it, e.g. to run a test copy next to your own). The one exception is the opt-in **Enable** button,
which sets `statusLine` in `~/.claude/settings.json` and nothing else.

### Current limitations
- `claude -p` is non-interactive, so tools that need your approval are refused (Settings →
  Extra arguments can pass a permission mode).
- You can't rename or reorder workspaces (sessions can be renamed).

Known gaps in the newer features are listed in [README.md](README.md#gaps).
Planned features for the next rounds are in [ROADMAP.md](ROADMAP.md).

## Code layout

```
src/
├── main.rs        window setup
├── app.rs         the panels, tabs, drag-and-drop, prompt input (queue, stop, refine), pinned prompt
├── session.rs     finds and parses Claude Code session files
├── history.rs     Up/Down prompt history
├── names.rs       custom session names and goals
├── goal.rs        goal + subgoal checklist format
├── diffview.rs    changes grouped by file, diff cards, read-only file viewer
├── markdown.rs    Markdown rendering: tables and tool lines drawn by scope, the rest by egui_commonmark
├── theme.rs       the colors for dark and light
├── settings.rs    the Settings page, its file, and the config folder
├── state.rs       what is restored after a restart; per-session views (file tabs, draft)
├── queue.rs       prompts waiting for a session's run
├── refine.rs      Refine prompt: the request to the helper model and its reply
├── runner.rs      runs `claude -p` (streaming send, stop, helper questions), folder picker, notifications
├── search.rs      session search: background index, matching and ranking, Haiku search format
├── status.rs      `scope statusline`: saves Claude Code's status data; usage limits
└── workspaces.rs  workspaces: create, move, remove, delete, load/save
```

Built with [egui/eframe](https://github.com/emilk/egui) (GUI), `egui_extras` + `syntect`
(syntax highlighting), `egui_commonmark` + `pulldown-cmark` (Markdown), `serde_json` (parsing),
`similar` (diffs), `objc2-app-kit` (the native folder picker; already used by the window
library), `libc` (stopping a run's process group).

```sh
cargo run --release   # start the app
cargo test            # parsing, history, workspaces, queue, state, refine, links, stop
```

## How the project is built

The project uses [Spec Kit](https://github.com/github/spec-kit). Every feature goes through
spec → plan → tasks → implementation, and the documents stay in the repo:

| Path | Contents |
|------|----------|
| `.specify/memory/constitution.md` | Ground rules: build the minimum, never write Claude's files, one binary, learn by using |
| `specs/001-session-manager-ui/` | Session list, prompt bar with history, diff panel |
| `specs/002-workspaces-pinned-prompt/` | Workspace tabs, drag-and-drop, Move to, pinned prompt |
| `specs/003-rename-sessions/` | Double-click to rename a session |
| `specs/004-workspace-colors-terminal-input/` | Workspace colors, terminal-style input at the bottom |
| `specs/005-new-session-click-focus/` | New session button, click anywhere to focus the input |
| `specs/006-right-panel-info-tab/` | Changes / Info tabs: model, context, goal, usage limits |
| `specs/007-goal-checklist-btw/` | Goal subgoal checklist, `/btw` side questions |
| `specs/008-goal-questions-in-info/` | Goal first in Info, its own question field (replaces `/btw`) |
| `specs/009-changes-by-file-file-tabs/` | Changes by file with highlighting, file list, read-only file tabs |
| `specs/010-markdown-rendering-themes/` | Markdown and tables in the conversation, rendered `.md` files, message cards, themes |
| `specs/011-search-live-progress-multiline/` | Session search (live + Haiku), live progress while Claude works, growing prompt input, no header |
| `specs/012-roadmap-rounds-1-4/` | The 12 items of [ROADMAP.md](ROADMAP.md): links, tabs, input, top bar, per-session state, restore, Ctrl+C, queue, Settings, Refine |

Each feature folder has `spec.md` (what and why), `plan.md` (how), and `tasks.md` (the
checklist that was implemented). `specs/001-session-manager-ui/research.md` lists open
questions to settle by using the app.

With Claude Code in this folder, the `/speckit-specify`, `/speckit-plan`, `/speckit-tasks` and
`/speckit-implement` skills run the same workflow for the next feature.
