# sessman

A small desktop app for browsing and continuing Claude Code sessions.

- **Left**: a **+ New session** button, then your sessions from `~/.claude/projects`, newest first, with workspace tabs (+ to create, drag or right-click → Move to)
- **Center bottom**: a terminal-style prompt input. Up/Down walks through this session's previous prompts; `/btw <question>` asks on the side without adding to the session; Enter sends the prompt via `claude -p --resume <id>`
- **Center**: the conversation (click anywhere to focus the input)
- **Right**: tabs. **Changes**: code changes made in the session. **Info**: model, context usage, session goal with a subgoal checklist, usage limits

```sh
cargo run --release
cargo test
```

Specs, plan, and tasks are in `specs/001-session-manager-ui/` (Spec Kit). The open questions to
settle by using the app are in `research.md`.

See [PROJECT.md](PROJECT.md) for a full explanation of the app, how it works, and how it is built.
