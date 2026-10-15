# sessman

A small desktop app for browsing and continuing Claude Code sessions.

- **Left**: your sessions from `~/.claude/projects`, newest first (⟳ to refresh)
- **Center top**: the prompt bar. Up/Down walks through this session's previous prompts; Enter sends the prompt via `claude -p --resume <id>`
- **Center**: the conversation
- **Right**: code changes (Edit / MultiEdit / Write) made in the session

```sh
cargo run --release
cargo test
```

Specs, plan, and tasks are in `specs/001-session-manager-ui/` (Spec Kit). The open questions to
settle by using the app are in `research.md`.
