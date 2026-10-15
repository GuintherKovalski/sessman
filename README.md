# sessman

A small desktop app for browsing and continuing Claude Code sessions.

- **Left**: a **+ New session** button, then your sessions from `~/.claude/projects`, newest first (refreshes on its own), with workspace tabs (+ to create, drag or right-click → Move to). **Search** at the bottom: type to filter by name, goal, then content; Enter asks Haiku
- **Center bottom**: a terminal-style prompt input that grows as you type (Shift+Enter for a new line). Up/Down walks through this session's previous prompts; Enter sends the prompt via `claude -p --resume <id>`, and Claude's progress streams in live
- **Center**: the conversation as Markdown cards, with real tables and highlighted code (click anywhere to focus the input); `.md` files open rendered
- **Theme**: dark / light / system switch next to the right panel's tabs
- **Right**: tabs. **Changes**: code changes grouped by file with syntax highlighting, and a file list (click to filter, double-click to open the file read-only in a main-area tab). **Info**: session goal with a subgoal checklist and a field to ask about it, model, context usage, usage limits

```sh
cargo run --release
cargo test
```

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

See [PROJECT.md](PROJECT.md) for a full explanation of the app, how it works, and how it is built.
