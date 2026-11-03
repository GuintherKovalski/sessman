# scope

A small desktop app for browsing and continuing Claude Code sessions.

- **Top**: a thin bar that holds the macOS window buttons (drag to move, double-click to zoom)
- **Left**: a **+ New session** button (native folder picker), then your sessions from `~/.claude/projects`, newest first (refreshes on its own), with workspace tabs (+ to create, drag or right-click → Move to). Shift-click (range), Cmd-click, or drag a box from an unselected row to pick several sessions; drag the picked ones onto a tab, or right-click them to move, remove or move to the Trash (Esc clears the picks). **Search** at the bottom: type to filter by name, goal, then content; Enter asks Haiku
- **Orchestration**: in each workspace tab, a bubble above its sessions (filled with the workspace color, with a ◆ mark and a progress line) opens the workspace's orchestration page: overall progress, what needs you, next steps, and a card per session (live state, what it is doing, its next step, goal checklist). The helper model writes the report and updates it with only the new activity; **Clear & re-summarize** starts it over from scratch. Ask the orchestrator questions at the bottom of the page
- **Center bottom**: a terminal-style prompt input that grows upwards as you type (Shift+Enter for a new line), with a drag handle to resize or collapse it. Up/Down walks through this session's previous prompts; Enter sends the prompt via `claude -p --resume <id>`, and Claude's progress streams in live. While Claude works, Enter queues prompts (shown above the input) and **Ctrl+C** stops it at once. **✨ / Cmd+Enter** (or **Cmd+R** from anywhere) refines the prompt with a helper model, asking skippable questions when it is ambiguous. **🎤 / Cmd+D** dictates: click (or press) to start, again to stop, and the speech is transcribed locally with Whisper and inserted at the cursor
- **Center**: the conversation as Markdown cards, with real tables, highlighted code and clickable links (bare URLs too); click anywhere to focus the input. Files open in tabs that belong to their session; `.md` files open rendered. **Ctrl+F** (or Cmd+F) finds text in whatever is on screen, the conversation or the open file: Enter / Shift+Enter step through the matches, Esc closes
- **Managed through Claude**: `scope list | rename | move | new | ask` drive the running app (sessions are named by id, id prefix or name). Every session scope runs is told about these commands, so you can ask Claude to start, rename or reorder sessions, or to ask the orchestrator. The orchestrator can do the same when you ask it on its page. On the orchestration page itself you can also rename sessions (✏ or right-click), drag cards to reorder the bar (**Newest first** undoes it), and click sessions mentioned in the report to open them
- **Settings** (⚙ or Cmd+,): appearance, prompt input, files, Claude command and models, behaviour, **harness** (editable instructions for workspace orchestration, the Info panel's questions and the session search, plus the orchestration model and update interval), shortcuts
- **Restart-proof**: the open workspace and session, tabs, drafts, queues and layout come back after a restart; each workspace reopens its last session
- **Right**: tabs. **Changes**: code changes grouped by file with syntax highlighting, and a file list (click to filter, double-click to open the file read-only in a main-area tab). **Info**: session goal with a subgoal checklist and a field to ask about it, model, context usage, usage limits

```sh
cargo run --release
cargo test
scripts/bundle.sh     # builds scope.app with its icon into ~/Applications (or a dir you pass)
```

**Testing changes**: always in a new instance of scope, never the one running your sessions
(restarting it ends them): `scripts/test-instance.sh start`, then `stop`. See
[CLAUDE.md](CLAUDE.md). Taking changes live without a restart is on the
[roadmap](ROADMAP.md#13-live-changes-scope-updates-without-ending-its-sessions).

Pin `~/Applications/scope.app` in the Dock, not `target/release/scope`: a bare binary shows
the generic executable icon whenever the app isn't running.

Dictation (the 🎤 button by the prompt) needs a few things on macOS, once:

- `brew install cmake`, which builds the Whisper engine (whisper.cpp, with Metal) for `cargo build`
- The Whisper model, about 150 MB, in `~/.config/scope/models/`:

  ```sh
  mkdir -p ~/.config/scope/models
  curl -L -o ~/.config/scope/models/ggml-base.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
  ```

The first recording asks for microphone access. Linux is not supported yet.

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
