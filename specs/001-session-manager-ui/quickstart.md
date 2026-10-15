# Quickstart

Prerequisites: Rust toolchain, `claude` CLI on PATH, at least one Claude Code session on disk.

```sh
cargo test     # parsing + history unit tests pass
cargo run      # window opens
```

Validate:
1. Left panel lists sessions, newest first (US1).
2. Click a session → messages appear in the center (US1).
3. Click the prompt input, press Up repeatedly → previous prompts in reverse order; Down returns to the draft (US2).
4. Type "say hi", press Enter → status shows running; reply appears when done (US2).
5. Open a session that edited files → right panel shows red/green diffs (US3).
