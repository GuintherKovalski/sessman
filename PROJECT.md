# sessman: project overview

sessman is a small desktop app, written in Rust, for browsing, organizing and continuing
your Claude Code sessions. It reads the sessions Claude Code already keeps on disk; it does
not replace Claude Code.

It is deliberately minimal: the goal is to use it, find out what feels right, and grow it from
there. Each change is specified first with Spec Kit (see [How the project is built](#how-the-project-is-built)).

## The window

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
- **+ New session** (below the tabs, above the list): pick the folder Claude should work in (macOS folder picker,
  starting at the current session's folder). The main area shows an empty new session; your
  first prompt creates it, and it then appears at the top of the list, selected. If a
  workspace tab is open, the new session joins that workspace.
- Lists every Claude Code session, newest first. Each row shows the session title (Claude's
  generated title, or your first prompt) and its project folder. The list refreshes on its
  own (every 10 s and after each reply), so sessions started elsewhere show up by themselves.
- **Search** (bottom of the panel): typing filters the list (within the open tab). Name
  matches come first, then goal summaries, then the conversations' own text (with a snippet
  under the name). Close spellings still match names and goals. **Enter** asks Claude Haiku
  (a little of your usage) which of the 40 most recent sessions match what you described,
  and lists its picks with a reason each. Editing the query goes back to live results;
  **Escape** clears it.
- Click a session to open it in the middle.
- **Double-click** a session name to rename it, like a folder: the name is selected, type the
  new one, **Enter** (or click elsewhere) saves, **Escape** cancels. Saving an empty name
  brings back the original one. Names are sessman's own; Claude Code keeps its own title.
- **Tabs** at the top: "All sessions", one tab per workspace, and **+**.
  - **+** asks for a name and creates a workspace tab.
  - Drag a session onto a workspace tab to put it there (the tab is outlined while you hover).
  - Or right-click a session → **Move to** → pick a workspace.
  - Inside a workspace tab, right-click a session → **Remove from workspace**.
  - Right-click a workspace tab → **Delete workspace** (the sessions themselves are untouched).
  - A session belongs to at most one workspace; "All sessions" always shows everything.
  - Every workspace has its own color (no two share one). Its tab is tinted with it, and in
    "All sessions" each session in a workspace gets a colored stripe and tint (the ▌ above).

### Middle: conversation and file tabs
- When a file is open, a tab bar appears on top: **Main** (the conversation) and one tab per
  open file, with the active one highlighted and an `x` to close. Files are **read-only**, with
  line numbers and syntax highlighting; text can be selected and copied. They reload whenever
  the session reloads, so they show Claude's latest changes. Markdown files (`.md`) open
  **rendered**; **Rendered / Source** at the top switches to the raw text.
- **Click anywhere** in the main area to put the cursor in the prompt input, like a terminal.
  Opening a session also focuses it.
- **Prompt input** at the bottom, styled like a terminal line (`>` prompt, monospace, dark
  background, no buttons). With it focused:
  - It grows as you type, up to about 10 lines, then scrolls. **Shift+Enter** starts a new line.
  - **Up / Down** step through the prompts you sent earlier in this session (like shell
    history) while the input is a single line. Going past the newest brings back what you
    were typing.
  - **Enter** sends the prompt to the session. It shows in the conversation at once, and
    Claude's progress follows live: the reply as it is written, a `⚙ Tool…` line when a tool
    starts, and each finished step (and its file changes, in the Changes tab) before the
    whole run ends.
  - Several sessions can run at once: while one works (⏳ next to its name in the list), you
    can open another, or start a new one, and send it a prompt. Switching back to a working
    session shows what it has done so far and keeps streaming.
- **Pinned "You"**: the prompt that the reply at the top of your view answers stays pinned
  above the conversation, so you always know what a long reply is about.
- The conversation itself: one card per message (blue accent for you, orange for Claude),
  rendered as Markdown: headings, lists, bold/italic, links (click to open in the browser),
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
- **Theme** (right of the tabs): 🌙 dark, ☀ light, 💻 follow the macOS appearance.
  Remembered between runs.
- **Info** (top to bottom):
  - **Goal**: until you click **Summarize goal** it shows the start of the first prompt. The
    button asks Claude Haiku (a little of your usage) for the goal plus a **checklist of 3–7
    subgoals**, ticked where the conversation shows they're done, with "N of M done". You can
    tick and untick items yourself; that is saved.
  - **Model** of the latest reply.
  - **Context**: tokens in context vs the window size (e.g. 204k / 1.0M).
  - **Usage limits**: your 5-hour and weekly limits with time until reset. Claude Code only
    shares these with its status line, so click **Enable** once: it sets sessman as Claude
    Code's status line (`statusLine` in `~/.claude/settings.json`). Limits then appear after
    your next reply in an interactive Claude Code session (Pro/Max plans). If you already
    have a status line, sessman leaves it alone.
  - **Ask about this goal**: the last item, pinned to the bottom of the panel. Enter sends the question, together
    with the goal and checklist, to the session's own model with its full context, like
    Claude Code's `/btw`: nothing is added to the session. The answer shows just above the field
    until you click Dismiss.

## How it works

| What | Where it comes from |
|------|---------------------|
| Session list and conversations | `~/.claude/projects/<project>/<session-id>.jsonl`, written by Claude Code. Read-only. |
| Session title | The `ai-title` entry in the session file, else the first prompt. |
| Prompt history | The prompts you typed in that session (tool results and slash-command wrappers are skipped). |
| Diffs | The `Edit` / `MultiEdit` / `Write` tool calls recorded in the session, diffed line by line. |
| Sending a prompt | Runs `claude -p --resume <session-id> --output-format stream-json --verbose --include-partial-messages "<prompt>"` in the session's folder, in the background; its streamed lines update the open conversation as they arrive. |
| New session | Runs `claude -p --session-id <new uuid> "<first prompt>"` in the chosen folder. |
| Model, context tokens | The latest reply's `model` and `usage` in the session file. |
| Context size, usage limits | Claude Code's status-line data, saved by `sessman statusline` to `~/.config/sessman/status/`. Without it, the size is inferred from the model. |
| Search | An in-memory index (names, goals, conversation text) rebuilt in the background for changed files every 10 s. Enter-search: `claude -p --model haiku --no-session-persistence` with a numbered list of recent sessions (name, goal, first prompts). |
| Goal and checklist | `claude -p --model haiku --no-session-persistence` with a digest of the conversation, on request; saved (with your ticks) to `~/.config/sessman/goals.json`. |
| Goal questions | `claude -p --resume <id> --fork-session --no-session-persistence "<question>"`: full context, original session untouched, nothing saved. |
| Theme | sessman's own file: `~/.config/sessman/settings.json`. |
| Workspaces and their colors | sessman's own file: `~/.config/sessman/workspaces.json`. |
| Custom session names | sessman's own file: `~/.config/sessman/names.json`. |

sessman never writes to Claude's session files. Everything it adds (workspaces, names, goals,
theme, status data) lives in its own config folder. The one exception is the opt-in **Enable** button,
which sets `statusLine` in `~/.claude/settings.json` and nothing else.

### Current limitations
- `claude -p` is non-interactive, so tools that need your approval are refused.
- You can't rename or reorder workspaces (sessions can be renamed).

Known gaps in the newer features are listed in [README.md](README.md#gaps).

## Code layout

```
src/
├── main.rs        window setup
├── app.rs         the three panels, tabs, drag-and-drop, context menus, pinned prompt
├── session.rs     finds and parses Claude Code session files
├── history.rs     Up/Down prompt history
├── names.rs       custom session names and goals
├── goal.rs        goal + subgoal checklist format
├── diffview.rs    changes grouped by file, diff cards, read-only file viewer
├── markdown.rs    Markdown rendering: tables and tool lines drawn by sessman, the rest by egui_commonmark
├── theme.rs       dark / light / system preference and the colors for each
├── runner.rs      runs `claude -p` (streaming send, new session, Haiku questions), folder picker
├── search.rs      session search: background index, matching and ranking, Haiku search format
├── status.rs      `sessman statusline`: saves Claude Code's status data; usage limits
└── workspaces.rs  workspaces: create, move, remove, delete, load/save
```

Built with [egui/eframe](https://github.com/emilk/egui) (GUI), `egui_extras` + `syntect`
(syntax highlighting), `egui_commonmark` + `pulldown-cmark` (Markdown), `serde_json` (parsing),
`similar` (diffs).

```sh
cargo run --release   # start the app
cargo test            # parsing, history and workspace tests
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

Each feature folder has `spec.md` (what and why), `plan.md` (how), and `tasks.md` (the
checklist that was implemented). `specs/001-session-manager-ui/research.md` lists open
questions to settle by using the app.

With Claude Code in this folder, the `/speckit-specify`, `/speckit-plan`, `/speckit-tasks` and
`/speckit-implement` skills run the same workflow for the next feature.
