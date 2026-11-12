# scope

**Organize your Claude Code sessions by scope, and stop holding all of them in your head.**

![scope in 20 seconds: scopes, session status, themes, dictation and refine](assets/scope.gif)

scope is a desktop IDE for working *with* Claude Code rather than typing code yourself. When
you run several sessions at once, the hard part is no longer the code: it is remembering
which session was doing what, which one finished, which one is waiting on you, and what each
one changed. Every switch between them costs you a re-read. scope carries that context for
you, so you can direct the work instead of reconstructing it.

```
  terminal 1: claude  "fix the login bug"     <- is it done?
  terminal 2: claude  "add CSV export"        <- waiting for me?
  terminal 3: claude  "refactor the parser"   <- what did it change?
  terminal 4: claude  ???                     <- what was this one about?
```

## How it works

- **Scopes group sessions by the work they belong to.** A scope is a tab (a feature, a
  ticket, a project) with its own color. Drag sessions into it, order them by hand, and it
  reopens where you left it. **SCOPES** shows every session and a card per scope: what the
  scope is about, its Jira tickets, and a tile per session saying what it is doing and what
  it needs from you.
- **Every session tells you where it stands.** While Claude works the session shows a
  spinner; when it stops, a small model reads the end of the run and tags it **✔ done**,
  **✋ needs you** or **⚠ blocked**, with the reason. Each session gets a short name and a goal
  with a checklist of subgoals, written for you after its first answer.
- **An orchestrator watches each scope.** Its page sums the scope up: overall progress, what
  needs you, the next steps, and a card per session. It updates with only the new activity,
  and you can ask it questions about the scope. Its **Processes** tab shows everything the
  sessions are running right now (commands, MCP servers, searches) with CPU, memory and time
  left.
- **You switch by clicking, not by re-reading.** Each session keeps its own draft, queued
  prompts, open files and scroll position; the prompt that the reply on screen answers stays
  pinned above it; the **Changes** tab shows exactly what Claude edited.
- **It is still Claude Code underneath.** scope reads the sessions Claude Code keeps in
  `~/.claude/projects` and continues them with `claude -p --resume`. Sessions you start in a
  terminal show up on their own, runs keep going when scope quits, and everything scope adds
  (scopes, names, goals, settings) lives in its own config folder.
- **Claude can drive it too.** Every session scope runs knows the `scope` command, so you can
  ask Claude to start, rename or reorder sessions, ask the orchestrator something, or restyle
  the app ("make it warmer").

## Install

Needs macOS 11 or later (Apple Silicon or Intel) and [Claude Code](https://docs.claude.com/en/docs/claude-code).
In a terminal:

```sh
curl -fsSL https://github.com/GuintherKovalski/sessman/releases/latest/download/install.sh | sh
```

It downloads the latest scope (checking its checksum and signature), puts it in /Applications,
downloads the speech model for dictation, installs Claude Code if it's missing, and opens scope.
Run it again any time to update. Or:

- **Homebrew**: `brew install --cask guintherkovalski/scope/scope`
- **By hand**: download `scope-<version>.dmg` from the [latest release](https://github.com/GuintherKovalski/sessman/releases/latest),
  open it and drag scope to Applications.

That's all: nothing to compile, no Rust or cmake. scope updates itself: when a new version is
out, the top bar says so, and **Settings → About** installs it and restarts (your sessions keep
running). Installed another way, dictation (🎤) offers to download its speech model (about
150 MB) the first time you use it; the first recording asks for microphone access.

## Features

**Sessions and scopes** (left)
- **+ New session** picks a folder and opens an empty session; the first prompt creates it.
  Model, permissions, thinking, MCP servers, skills, agent, hooks and plugins can be picked
  for it from dropdowns.
- Sessions newest first, refreshed on their own, each with its state (working, queued
  prompts, done / needs you / blocked). Double-click to rename.
- Scope tabs: **+** to create; drag sessions onto a tab or right-click → **Move to**; drag
  tabs to reorder; right-click a tab to rename, recolor or delete it.
- Inside a scope, drag sessions to arrange them (**Sort newest first** undoes it).
  Shift-click, Cmd-click or a drawn box picks several, to move, remove or trash at once.
  Delete takes sessions out of the scope; **Cmd+Z** puts them back.
- Up / Down walk the list once a session is clicked; typing goes to the prompt.
- **Search**: filters by name, then goal, then conversation text; **Enter** asks Haiku to
  find sessions matching a description.

**Overview and orchestration** (main area)
- **SCOPES** overview: a card per scope (its purpose, Jira tickets, a tile per session with
  what it is doing and what it needs), most recently active first. Long-idle sessions offer
  **Finish session**.
- The **◆ Orchestration** bubble at the top of each scope shows its progress and opens its
  page: summary, **Needs you**, **Next**, session cards you can reorder and rename, links to
  every session mentioned, and an **Ask** field. **Clear & re-summarize** starts over.
- **Processes**: what each working session is running, as a tree, with a stop button per row.

**Conversation and prompt** (center)
- The conversation as Markdown cards: real tables, highlighted code, clickable links, image
  thumbnails. Each card has **Copy** and **🗑** (removes the message from Claude's context).
  Click a message, then Up / Down to read back one message at a time.
- A terminal-style prompt that grows upwards (Shift+Enter for a new line) and can be resized
  or collapsed. Up / Down for this session's earlier prompts. **Cmd+V** pastes images.
- **Enter** sends and streams Claude's progress live. While it works, Enter **queues**
  prompts; **Ctrl+C** stops the run at once and pauses the queue.
- **✨ Refine** (Cmd+Enter or Cmd+R) rewrites the prompt with a helper model, asking
  skippable questions when it is ambiguous. **🎤** dictates, transcribed locally with Whisper.
- Several sessions run at once; switching back to a working one keeps streaming.
- Files open read-only in tabs that belong to their session; `.md` files open rendered.
  **Cmd+F** finds text in the conversation or the open file.

**Changes, Info and Templates** (right)
- **Changes**: every edit Claude made, grouped by file with syntax-highlighted diffs; click a
  file to filter, double-click to open it.
- **Info**: the goal and its subgoal checklist, the model (switchable between runs), context
  usage, your 5-hour and weekly limits, and **Ask about this goal** (a side question that
  doesn't touch the session).
- **Templates**: the scope's prompt templates, skills and commands (yours and every
  subproject's). **Use in prompt** inserts one; **✨ Create template** has Claude write a new
  one, and `@[session]` turns what a session did into a reusable prompt.

**Everywhere**
- **Settings** (⚙ or Cmd+,): appearance and style presets, prompt input, Claude command and
  models, behaviour, and every helper's **harness** (the instructions it starts from, editable,
  with the full prompt it sends).
- **Driven by commands**: `scope list | rename | move | new | ask | style` run against the
  open app, from a terminal or from any Claude session.
- **Restart-proof**: scopes, open session, tabs, drafts, queues and layout come back after a
  restart, and runs keep going while scope is closed.
- **Several windows** on the same config stay in sync within half a second.
- **Updates itself**: the top bar says when a new version is out; Settings → About installs it
  and restarts without ending your sessions.

## Build from source

Needs [Rust](https://rustup.rs) and `brew install cmake` (it builds the Whisper engine,
whisper.cpp with Metal, for dictation).

```sh
cargo run --release
cargo test
scripts/bundle.sh     # builds scope.app with its icon into ~/Applications (or a dir you pass)
scripts/release.sh    # the release files (universal .dmg and .zip) into dist/
```

Pin `~/Applications/scope.app` in the Dock, not `target/release/scope`: a bare binary shows
the generic executable icon whenever the app isn't running.

**Testing changes**: always in a new instance of scope, never the one running your sessions
(restarting it ends them): `scripts/test-instance.sh start`, then `stop`. See
[CLAUDE.md](CLAUDE.md). Several features at once, each with its own worktree, build and test
window: `scripts/feature.sh`, see [WORKTREES.md](WORKTREES.md). Taking changes live without a restart is on the
[roadmap](ROADMAP.md#13-live-changes-scope-updates-without-ending-its-sessions).

**Releasing**: `scripts/flow.sh release 0.2.0`; CI builds, signs, notarizes and publishes it.
The one-time setup (Developer ID certificate, secrets, Homebrew tap) is in [RELEASING.md](RELEASING.md).

Linux is not supported yet.

Specs, plan, and tasks are in `specs/001-session-manager-ui/` (Spec Kit). The open questions to
settle by using the app are in `research.md`.

## Gaps

Known shortcomings of what's already built, to pick up as Spec Kit features later.

### Markdown rendering (`specs/010-markdown-rendering-themes/`)

- **Table cells can't be selected.** Tables are drawn by hand (to get column alignment and
  sideways scrolling), so their text can't be selected or copied like the rest of a message.
- **Only the first link in a table cell opens.** Links in cells are colored and underlined,
  but clicking anywhere in the cell opens its first link; a cell with several links can't
  open the others.
- **Opening a session draws every message once.** Off-screen messages are skipped only after
  their height is known, so the first frame after opening (or reloading, or resizing) a
  session draws all of them. It may stutter briefly on very long sessions.

### Search and live progress (`specs/011-search-live-progress-multiline/`)

- **Content search needs exact words.** Typos are tolerated in names and goals, but words
  inside conversations must appear as typed (case-insensitive).
- **Content search can lag up to 10 s.** The index is rebuilt in the background; text from the
  last few seconds of another session may not be found yet.
- **Haiku search sees only the 40 most recent sessions**, and only their names, goals and
  first three prompts, not the whole conversation.
- **Sub-agent work isn't shown live.** While a sub-agent (Task tool) runs, only its tool line
  shows; its own steps don't stream in.
- **Switching back to a running session can show a step twice** until the run ends (the step
  was already on disk when the session reloaded and then arrived from the stream too); the
  reload at the end of the run fixes it.

### Roadmap rounds 1–4 (`specs/012-roadmap-rounds-1-4/`)

- **Testing with synthetic key events**: modifier keys sent by System Events (e.g. Shift+Enter)
  sometimes arrive without the modifier, so automated UI checks use CGEvents with explicit
  flags. Real keyboards are not affected.
- **A few symbols** (→, ▾) are missing from egui's default font and render as boxes; scope
  avoids them in its own text (the input's collapse triangle is drawn), but they still show
  as boxes inside conversations that contain them.
- **A stopped run may add a "No response requested." reply** the next time the session runs:
  that comes from Claude Code itself, after its interrupt marker.
- **Selecting a very long session briefly blocks the window** while it loads (sessions are
  read on the UI thread).

See [PROJECT.md](PROJECT.md) for a full explanation of the app, how it works, and how it is built.
