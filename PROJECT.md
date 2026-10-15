# sessman: project overview

sessman is a small desktop app, written in Rust, for browsing, organizing and continuing
your Claude Code sessions. It reads the sessions Claude Code already keeps on disk; it does
not replace Claude Code.

It is deliberately minimal: the goal is to use it, find out what feels right, and grow it from
there. Each change is specified first with Spec Kit (see [How the project is built](#how-the-project-is-built)).

## The window

```
┌──────────────────┬──────────────────────────────────────┬─────────────────────┐
│ Sessions ⟳       │ [ prompt bar               ] [Send]  │ Changes             │
│ All sessions │ A │ ┌──────────────────────────────────┐ │ src/app.rs          │
│ │ +            │ │ You  (pinned prompt)             │ │ - old line          │
│──────────────────│ └──────────────────────────────────┘ │ + new line          │
│ Session title    │ You                                  │                     │
│ project folder   │ ...                                  │ src/main.rs         │
│ Session title    │ Claude                               │ + added line        │
│ ...              │ ...                                  │ ...                 │
└──────────────────┴──────────────────────────────────────┴─────────────────────┘
```

### Left: sessions and workspaces
- Lists every Claude Code session, newest first. Each row shows the session title (Claude's
  generated title, or your first prompt) and its project folder. ⟳ reloads the list.
- Click a session to open it in the middle.
- **Tabs** at the top: "All sessions", one tab per workspace, and **+**.
  - **+** asks for a name and creates a workspace tab.
  - Drag a session onto a workspace tab to put it there (the tab is outlined while you hover).
  - Or right-click a session → **Move to** → pick a workspace.
  - Inside a workspace tab, right-click a session → **Remove from workspace**.
  - Right-click a workspace tab → **Delete workspace** (the sessions themselves are untouched).
  - A session belongs to at most one workspace; "All sessions" always shows everything.

### Middle: conversation
- **Prompt bar** at the top, always visible. With it focused:
  - **Up / Down** step through the prompts you sent earlier in this session (like shell
    history). Going past the newest brings back what you were typing.
  - **Enter** (or Send) sends the prompt to the session. A spinner shows while Claude works;
    the conversation reloads when it is done.
- **Pinned "You"**: the prompt that the reply at the top of your view answers stays pinned
  above the conversation, so you always know what a long reply is about.
- The conversation itself: your prompts in blue, Claude's text in orange, tool calls shown as
  `[tool: Name]`.

### Right: changes
Every file change Claude made in the session (Edit, MultiEdit, Write), in order, with the
file path and removed (red `-`) / added (green `+`) lines. Long diffs are cut at 200 lines.

## How it works

| What | Where it comes from |
|------|---------------------|
| Session list and conversations | `~/.claude/projects/<project>/<session-id>.jsonl`, written by Claude Code. Read-only. |
| Session title | The `ai-title` entry in the session file, else the first prompt. |
| Prompt history | The prompts you typed in that session (tool results and slash-command wrappers are skipped). |
| Diffs | The `Edit` / `MultiEdit` / `Write` tool calls recorded in the session, diffed line by line. |
| Sending a prompt | Runs `claude -p --resume <session-id> "<prompt>"` in the session's folder, in the background. |
| Workspaces | sessman's own file: `~/.config/sessman/workspaces.json`. |

sessman never writes to Claude's files. Everything it adds (workspaces) lives in its own config file.

### Current limitations
- Replies appear when complete, not word by word.
- `claude -p` is non-interactive, so tools that need your approval are refused.
- You can't start a new session from the app, or rename or reorder workspaces.
- The list does not refresh on its own; use ⟳.

## Code layout

```
src/
├── main.rs        window setup
├── app.rs         the three panels, tabs, drag-and-drop, context menus, pinned prompt
├── session.rs     finds and parses Claude Code session files
├── history.rs     Up/Down prompt history
├── runner.rs      runs `claude -p --resume` on a background thread
└── workspaces.rs  workspaces: create, move, remove, delete, load/save
```

Built with [egui/eframe](https://github.com/emilk/egui) (GUI), `serde_json` (parsing),
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

Each feature folder has `spec.md` (what and why), `plan.md` (how), and `tasks.md` (the
checklist that was implemented). `specs/001-session-manager-ui/research.md` lists open
questions to settle by using the app.

With Claude Code in this folder, the `/speckit-specify`, `/speckit-plan`, `/speckit-tasks` and
`/speckit-implement` skills run the same workflow for the next feature.
