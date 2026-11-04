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
it to zoom. At its right end, a dimmed **⚙** opens Settings.

```
 ● ● ●                                                                         ⚙
┌──────────────────┬──────────────────────────────────────┬─────────────────────┐
│ All │ A │ B │ +  │ ┌──────────────────────────────────┐ │ Changes │ Info      │
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
  One that has stopped shows how its work ended, beside its time: **✔ done**, **✋ needs you**
  or **⚠ blocked** (the reason on hover; `scope list` adds `[done]`, `[needs you]`,
  `[blocked]`). After each run ends, the helper model reads the end of the conversation and
  decides; scope also checks, a couple at a time, the workspace sessions changed in the last 3
  days that no verdict has read (runs that ended while it was closed, or in a terminal). Any
  newer activity drops the tag until it is checked again. Settings → Tag finished sessions
  turns it off.
- **⚙** (right end of the top bar, or **Cmd+,**) opens the Settings page in the main area.
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
- **Tabs** at the top: **SCOPES** (all sessions), one tab per workspace, and **+**. A
  workspace's name always shows in upper case (in its tab, its overview card, its
  orchestration page and the Move to menu); it is kept as typed, and found by it. Selecting a workspace tab
  opens the session that was last open in it (or its newest session; an empty workspace shows
  an empty main area); each remembers its last session across restarts. "All sessions" opens
  on the workspaces overview.
  - **+** asks for a name and creates a workspace tab.
  - Drag a session onto a workspace tab to put it there (the tab is outlined while you hover).
  - Inside a workspace tab (with no search), drag highlighted sessions (the open one, or the
    picked ones) up or down the list to reorder it: a line shows where they go. The order is
    saved (`bar` in `workspaces.json`, `scope list` says "arranged by hand"); sessions not
    placed yet, like new ones, go above the placed ones, newest first. Right-click a session →
    **Sort newest first** goes back. A drag from a row that isn't highlighted still draws a box.
  - Drag a workspace tab before or after another to reorder the tabs (a line shows where).
  - Or right-click a session → **Move to** → pick a workspace.
  - Inside a workspace tab, right-click a session → **Remove from workspace**.
  - Right-click a workspace tab → **Rename workspace**: the tab turns into a field with the
    name selected; **Enter** (or clicking away) saves, **Escape** cancels. A name another
    workspace has is refused. Its overview summary and orchestration keep following it.
  - Right-click a workspace tab → **Color**: pick one of the palette's swatches, or any color
    in the picker below them.
  - Right-click a workspace tab → **Delete workspace** (asks first, unless turned off in
    Settings; the sessions themselves are untouched).
  - A session belongs to at most one workspace; "All sessions" always shows everything.
  - Every workspace has its own color (no two share one). Its tab is tinted with it, and in
    "All sessions" each session in a workspace gets a bar in its color down its left edge and
    is otherwise drawn like any other (the ▌ above).

### Middle: workspaces overview (All sessions)
In "All sessions", the main area shows an overview of the workspaces until a session is
opened; clicking "All sessions" again comes back to it.
- One card per workspace, the one whose session went idle most recently first (sessions
  still working don't count; workspaces with none idle come last). Its color shows only as a
  stripe down the card's left edge and on its name, which is drawn exactly as its tab at the
  top left; cards are full width, with a grey border and no fill. Click the name to open the
  workspace. Under it, its **scope of work** in one short sentence,
  in a larger font, and up to 3 Jira tickets, epics and parent tickets first (only keys
  that really appear in the sessions or the branch; with a Jira URL in Settings they link to
  the ticket).
- Then **every session** of the workspace, strictly newest first, as a grid of equal
  tiles read row by row. Every card has the same number of columns: as many as fit when each
  is as wide as the longest short session name shown, at least 3. Short names (at most 4
  words, as Haiku names sessions) never wrap or get cut; a longer one, the first line of a
  prompt not renamed yet, doesn't count and is cut at the tile's edge. Tiles in a row
  are the same height. They are drawn like the sessions in the list
  (same fill, grey border and roundness). Each shows its name in a larger font, then
  in small print what it is doing and, in bold after ⚠, any decision or action it needs from
  you. At its bottom right, small: `14:32 · 5 min ago` (the time as the list shows it), or a spinner while Claude works in
  it.
- A session with no activity for 24 hours (Settings → Overview, 0 = never) shows **Finish
  session**: it leaves the workspace (the session itself is kept).
- A session whose run ended after you last had it open is marked
  **✔ Finished** (it is not moved: the order stays newest first); opening it clears the mark.
  What was last seen is kept in `state.json`.
- Only two small marks are colored, in the colors the orchestration page uses for the same
  states: what a session needs from you in its gold, **✔ Finished** in its green.
- Click a session to open it, in its workspace's tab.
- What the helper model made of each workspace is kept in `~/.config/scope/overview.json`.
  It is written when missing, then again (a few minutes apart at most) for the sessions with
  activity it hasn't seen once their run is over, up to 10 at a time; it keeps what it said of
  the others. **↻** on a card, or **↻ Refresh all**, asks now.

### Workspace orchestration
- In a workspace tab, the first bubble in the list is the workspace itself: **Orchestration**,
  drawn unlike the session bubbles (filled with the workspace color, a thicker outline, a ◆ mark,
  a line like `6 sessions · 2 working · 62% · 1 needs you`, and the progress along its bottom).
- Clicking it opens the orchestration page in the main area (the tab reopens on it; an empty
  workspace opens on it too): overall **progress**, a summary, **Needs you**, **Next**, then a
  card per session with its live state (working spinner and tool, queued prompts, last activity,
  goal checklist) and what the report says (working / waiting / blocked / done / idle, what it is
  doing, its next step). Click a card to open the session; drag it onto another to put it
  before or after that one (a line shows which side). The cards' order is their own: the bar's
  is arranged apart. Working is a live fact: a stopped
  session's card takes its state from its tag (and the tag's reason when the report hasn't read
  its last activity yet), never from a report written while it was still running.
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
- **Report / Processes** tabs at the top of the page. **Processes** (`Processes · N` when
  something runs) lists what the workspace's sessions are running right now, refreshed every
  2 s: one card per working session (click its name to open it) with its `claude` and, as a
  tree under it, everything it started: **Bash** commands (with Claude's description of each),
  **MCP** servers, **search** (ripgrep for Grep / Glob) and any other process. Each row shows
  CPU (of one core), RAM, how long it has been running and, for a Bash command, how much of
  its time limit is left (2 min unless the call set one; a background command has none).
  That limit is the only end known in advance: how far a command itself has got isn't.

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
- **Click anywhere** else in the main area to put the cursor in the prompt input, like a
  terminal. Opening a session also focuses it.
- **Click a message** in the conversation to pick it (outlined): the conversation then has the
  keyboard, and **Up / Down** move to the previous / next message, scrolling its start to the
  top, to read back through a long session one message at a time. Dragging still selects text.
  **Esc**, typing, or a click elsewhere hands the keyboard back to the input (typing goes in).
- **Prompt input** at the bottom, styled like a terminal line (`>` prompt, monospace). With it
  focused:
  - It grows **upwards** as you type, up to 10 lines (a setting), then scrolls; the
    conversation above shrinks. **Shift+Enter** starts a new line.
  - The short bar on its top edge is a drag handle, like the side panels': drag it to make the
    input taller or shorter; double-click it for the automatic size. The ▾ at its right
    collapses the input to one line (▴ expands it). The size is kept across restarts.
  - **Images**: **Cmd+V** pastes an image (a screenshot, Copy Image, or image files copied
    in Finder), and image files dropped on the window join the prompt too. Each goes in at
    the cursor as a marker, `[image:1a2b3c4d.png]`, shown as a chip; type around it as you
    like, the image is sent where its marker is. Thumbnails of the prompt's images sit above
    the text: hover for a bigger view, click to open it in Preview, `×` takes it out (its
    marker too). Big or unusual images (TIFF, BMP, HEIC, over 2000 px) are converted first.
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
- Images in your prompts (also ones pasted in a terminal Claude Code) show as thumbnails
  where they were; hover for a bigger view, click to open.
- The conversation itself: one card per message (blue accent for you, orange for Claude),
  rendered as Markdown: headings, lists, bold/italic, links, including bare URLs like
  `https://…` and `www.…` (highlighted, underlined on hover, click to open in the browser),
  quotes, code blocks with syntax highlighting, and **tables** as real tables (bold header,
  aligned columns, scroll sideways when wide). Tool calls are a small dimmed line
  (`⚙ Read · Edit`). Only messages near the visible area are drawn, so long sessions scroll
  smoothly.
- At the top right of every You / Claude card, two small buttons:
  - **Copy** (two overlapping squares) copies the whole message as its Markdown source
    (Claude's without its tool lines); it shows a check mark for a moment after.
  - **🗑** deletes the message **from the context**: Claude no longer sees it from the next
    prompt on. A reply goes with all its tool calls and their results. Files it changed stay
    as they are. It asks first (Settings → Behaviour can turn that off) and is disabled while
    Claude works in the session. Two prompts left next to each other merge into one card, as
    consecutive prompts always do.

### Right: Changes and Info tabs
- **Changes**: one card per changed file, in order of its first change: file name, folder,
  and `+added -removed` counts, then each edit as diff hunks with syntax highlighting, green /
  red tinted rows for added / removed lines, and unchanged stretches collapsed to 3 lines of
  context (`⋮` marks a gap). At the bottom, **Files** lists every changed file
  with its folder (relative to the session's, in full if it's outside it):
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
- **Updates** (under About): scope looks for a newer GitHub release at start and every 12 hours
  (only when it runs as an installed scope.app; off with *Check for updates automatically*).
  When there is one, the top bar shows *scope X is available* on the right, which opens
  Settings; **Install X and restart** downloads the release's `.zip`, checks its signature
  (same Developer ID team as the running app), swaps it in place of scope.app and restarts
  into it. Quitting doesn't end the sessions scope runs; the new scope picks them up.

### Restarting
Closing and opening scope brings everything back: window size and position, panel widths,
the input's size, the workspace tab and open session, every session's file tabs, scroll
positions, drafts and queued prompts (queues come back paused), the right panel tab and the
search text. Sessions or files that no longer exist are skipped; a damaged state file is
moved aside (`state.json.bad`) and the app starts fresh.

### Several windows
Every scope running on the same config folder (two windows, or one started while another
runs) shows the same workspaces (members, order, names, colors), session names, goals,
settings, overviews and orchestrations: what one saves, the others take within half a second,
without a restart. A change is made on top of what the others saved, so two windows never
undo each other's work. `scope` commands are run by whichever window takes them first, once.
What each window shows (its open tab and session, panels, drafts) stays its own.

## How it works

| What | Where it comes from |
|------|---------------------|
| Session list and conversations | `~/.claude/projects/<project>/<session-id>.jsonl`, written by Claude Code. Read-only, but for Move to Trash and deleting a message. |
| Session title | The `ai-title` entry in the session file, else the first prompt. |
| Prompt history | The prompts you typed in that session (tool results and slash-command wrappers are skipped). |
| Diffs | The `Edit` / `MultiEdit` / `Write` tool calls recorded in the session, diffed line by line. |
| Sending a prompt | Runs `claude -p [extra args] --resume <session-id> [--model <m>] --output-format stream-json --verbose --include-partial-messages "<prompt>"` in the session's folder, in its own process group, in the background; its streamed lines update the open conversation as they arrive. |
| Images in prompts | Kept in `~/.config/scope/images/`, named after their data, and written in the prompt as `[image:<name>]`. A prompt with images goes to `claude -p --input-format stream-json` on stdin, as one user message of text and image blocks in order. Claude Code keeps them in the session file; reading it back turns them into the same markers (and saves the files of images pasted elsewhere). Cmd+V is seen through an AppKit key monitor (egui only hears about pastes with text); the image comes from the macOS pasteboard. |
| Runs scope did not start | Every 2 s `ps` lists `claude -p` processes on a listed session (`--resume` / `--session-id <id>`) that aren't scope's own children: runs a scope left going when it quit (`claude` finishes and saves its reply with no one reading its output), or ones another scope or a terminal started. Each shows as working; its steps are read from the session file as Claude saves them (whole steps, no letter-by-letter text), until its process ends. Stop, the queue and notifications work as for scope's own runs. So quitting or restarting scope loses no run. |
| Processes tab | Every 2 s while the orchestration page is open, `ps -axww -o pid,ppid,pgid,%cpu,rss,etime,args`. A session's `claude` is one with `--resume` / `--session-id <id>` of a session in the workspace (side questions don't count); under it, its descendants and what kept its process group. A Bash command is the `eval '…'` in Claude Code's shell (it sources `shell-snapshots/`), matched to its `Bash` call in the session file for the description, `timeout` and `run_in_background`. |
| Stopping (Ctrl+C) | SIGINT to that process group (SIGKILL after 2 s). Claude Code saves the partial reply and an `[Request interrupted by user]` marker, shown as ⏹ Interrupted. |
| Queue | Kept per session in memory and in `state.json`; the next prompt is sent when a run ends normally. |
| Workspaces overview | `claude -p --model <helper> --no-session-persistence` with a digest of the workspace's 8 newest sessions (title, goal, conversation), the git branch and the Jira keys found in them; it answers in JSON with purpose, now, heading and tickets. |
| Refine | `claude -p --model <helper> --no-session-persistence` with the prompt, the goal and the last 3 prompts; it answers in JSON with either the refined prompt or up to 3 questions. |
| New session | Runs `claude -p --session-id <new uuid> "<first prompt>"` in the chosen folder. |
| Model, context tokens | The latest reply's `model` and `usage` in the session file. |
| Context size, usage limits | Claude Code's status-line data, saved by `scope statusline` to `~/.config/scope/status/`. Without it, the size is inferred from the model. |
| Search | An in-memory index (names, goals, conversation text) rebuilt in the background for changed files every 10 s. Enter-search: `claude -p --model haiku --no-session-persistence` with a numbered list of recent sessions (name, goal, first prompts). |
| Session tags | `claude -p --model haiku --no-session-persistence` with the end of the conversation (about 4000 characters) after each run, and in a sweep every 10 s (two at a time) for workspace sessions with unjudged activity from the last 3 days that have been quiet for a minute. Saved by session id, with the change it read, to `~/.config/scope/verdicts.json` (shared with other scope windows, like names); a failed check isn't retried until the session changes again. |
| Goal and checklist | `claude -p --model haiku --no-session-persistence` with a digest of the conversation, on request; saved (with your ticks) to `~/.config/scope/goals.json`. |
| Goal questions | `claude -p --resume <id> --fork-session --no-session-persistence "<question>"`: full context, original session untouched, nothing saved. |
| Deleting a message | Its span in the session file: from its first entry to the next message's. The `user` / `assistant` entries there are removed (attachments and Claude Code's own records stay), and entries that pointed at a removed one (`parentUuid`, `logicalParentUuid`, `leafUuid`) point at what it pointed at. Refused if the message changed since it was shown, or if it would split a tool call from its result (an interrupted run can leave the next prompt between them). The file as it was is copied to `~/.config/scope/backups/<id>-<ms>.jsonl` first. |
| Settings (theme included) | scope's own file: `~/.config/scope/settings.json`. |
| App state | `~/.config/scope/state.json` (sessions' tabs, drafts, queues, layout) and `window.ron` (window, panel sizes, scroll positions, saved by eframe). Saved every 2 s when changed and on quit. |
| Bare URLs | Turned into Markdown autolinks before rendering (not inside code or existing links). |
| Workspaces and their colors | scope's own file: `~/.config/scope/workspaces.json`. |
| Custom session names | scope's own file: `~/.config/scope/names.json`. |

scope writes to Claude's session files only when you ask it to, with **Move to Trash** and a
message's **🗑**, which keeps a backup first. Everything it adds (workspaces, names, goals,
settings, app state, status data) lives in its own config folder (`$SCOPE_CONFIG` overrides
it, e.g. to run a test copy next to your own). Its only other write outside that folder is the
opt-in **Enable** button, which sets `statusLine` in `~/.claude/settings.json` and nothing else.

### Current limitations
- `claude -p` is non-interactive, so tools that need your approval are refused (Settings →
  Extra arguments can pass a permission mode).

Known gaps in the newer features are listed in [README.md](README.md#gaps).
Planned features for the next rounds are in [ROADMAP.md](ROADMAP.md).

## Code layout

```
src/
├── main.rs        window setup
├── app.rs         the panels, tabs, drag-and-drop, prompt input (queue, stop, refine), pinned prompt
├── session.rs     finds and parses Claude Code session files
├── history.rs     Up/Down prompt history
├── images.rs      images in prompts: markers, the images folder, clipboard, stream-json message
├── names.rs       custom session names and goals
├── goal.rs        goal + subgoal checklist format
├── diffview.rs    changes grouped by file, diff cards, read-only file viewer
├── markdown.rs    Markdown rendering: tables and tool lines drawn by scope, the rest by egui_commonmark
├── theme.rs       the colors for dark and light
├── settings.rs    the Settings page, its file, and the config folder
├── shared.rs      config files every scope on one config folder shares: refresh, save, watch
├── state.rs       what is restored after a restart; per-session views (file tabs, draft)
├── queue.rs       prompts waiting for a session's run
├── processes.rs the orchestration page's Processes tab: ps, process trees, Bash calls, time limits
├── refine.rs      Refine prompt: the request to the helper model and its reply
├── runner.rs      runs `claude -p` (streaming send, stop, helper questions), folder picker, notifications
├── search.rs      session search: background index, matching and ranking, Haiku search format
├── status.rs      `scope statusline`: saves Claude Code's status data; usage limits
├── verdict.rs     session tags: done / needs you / blocked, judged by the helper model
├── update.rs      finds newer releases on GitHub, installs one in place of scope.app
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
