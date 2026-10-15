# sessman: project overview

sessman is a small desktop app, written in Rust, for browsing, organizing and continuing
your Claude Code sessions. It reads the sessions Claude Code already keeps on disk; it does
not replace Claude Code.

It is deliberately minimal: the goal is to use it, find out what feels right, and grow it from
there. Each change is specified first with Spec Kit (see [How the project is built](#how-the-project-is-built)).

## The window

```
┌──────────────────┬──────────────────────────────────────┬─────────────────────┐
│ Sessions ⟳       │ ┌──────────────────────────────────┐ │ Changes │ Info      │
│ All │ A │ B │ +  │ │ You  (pinned prompt)             │ │ src/app.rs          │
│ [+ New session ] │ └──────────────────────────────────┘ │ - old line          │
│▌Session title    │ You                                  │ + new line          │
│▌project folder   │ ...                                  │                     │
│ Session title    │ Claude                               │ src/main.rs         │
│ ...              │ ...                                  │ + added line        │
│                  │ ┌──────────────────────────────────┐ │                     │
│                  │ │ > type a prompt_                 │ │ ...                 │
│                  │ └──────────────────────────────────┘ │                     │
└──────────────────┴──────────────────────────────────────┴─────────────────────┘
```

### Left: sessions and workspaces
- **+ New session** (below the tabs, above the list): pick the folder Claude should work in (macOS folder picker,
  starting at the current session's folder). The main area shows an empty new session; your
  first prompt creates it, and it then appears at the top of the list, selected. If a
  workspace tab is open, the new session joins that workspace.
- Lists every Claude Code session, newest first. Each row shows the session title (Claude's
  generated title, or your first prompt) and its project folder. ⟳ reloads the list.
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
  the session reloads, so they show Claude's latest changes.
- **Click anywhere** in the main area to put the cursor in the prompt input, like a terminal.
  Opening a session also focuses it.
- **Prompt input** at the bottom, styled like a terminal line (`>` prompt, monospace, dark
  background, no buttons). With it focused:
  - **Up / Down** step through the prompts you sent earlier in this session (like shell
    history). Going past the newest brings back what you were typing.
  - **Enter** sends the prompt to the session. A spinner shows while Claude works;
    the conversation reloads when it is done.
- **Pinned "You"**: the prompt that the reply at the top of your view answers stays pinned
  above the conversation, so you always know what a long reply is about.
- The conversation itself: your prompts in blue, Claude's text in orange, tool calls shown as
  `[tool: Name]`.

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
| Sending a prompt | Runs `claude -p --resume <session-id> "<prompt>"` in the session's folder, in the background. |
| New session | Runs `claude -p --session-id <new uuid> "<first prompt>"` in the chosen folder. |
| Model, context tokens | The latest reply's `model` and `usage` in the session file. |
| Context size, usage limits | Claude Code's status-line data, saved by `sessman statusline` to `~/.config/sessman/status/`. Without it, the size is inferred from the model. |
| Goal and checklist | `claude -p --model haiku --no-session-persistence` with a digest of the conversation, on request; saved (with your ticks) to `~/.config/sessman/goals.json`. |
| Goal questions | `claude -p --resume <id> --fork-session --no-session-persistence "<question>"`: full context, original session untouched, nothing saved. |
| Workspaces and their colors | sessman's own file: `~/.config/sessman/workspaces.json`. |
| Custom session names | sessman's own file: `~/.config/sessman/names.json`. |

sessman never writes to Claude's session files. Everything it adds (workspaces, names, goals,
status data) lives in its own config folder. The one exception is the opt-in **Enable** button,
which sets `statusLine` in `~/.claude/settings.json` and nothing else.

### Current limitations
- Replies appear when complete, not word by word.
- `claude -p` is non-interactive, so tools that need your approval are refused.
- You can't rename or reorder workspaces (sessions can be renamed).
- The list does not refresh on its own; use ⟳.

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
├── runner.rs      runs `claude -p` (send, new session, goal summary), folder picker
├── status.rs      `sessman statusline`: saves Claude Code's status data; usage limits
└── workspaces.rs  workspaces: create, move, remove, delete, load/save
```

Built with [egui/eframe](https://github.com/emilk/egui) (GUI), `egui_extras` + `syntect`
(syntax highlighting), `serde_json` (parsing), `similar` (diffs).

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

Each feature folder has `spec.md` (what and why), `plan.md` (how), and `tasks.md` (the
checklist that was implemented). `specs/001-session-manager-ui/research.md` lists open
questions to settle by using the app.

With Claude Code in this folder, the `/speckit-specify`, `/speckit-plan`, `/speckit-tasks` and
`/speckit-implement` skills run the same workflow for the next feature.
