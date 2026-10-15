# Research: Claude Session Manager UI

## GUI toolkit
- **Decision**: egui via eframe.
- **Rationale**: "Click on a session" implies a mouse-driven window; egui gives side panels, scroll areas and text input with very little code and no web stack.
- **Alternatives considered**: ratatui TUI (mouse support exists but the three-column layout with diffs is cramped in a terminal); Tauri (adds a web frontend, violates Single Binary spirit); iced (more boilerplate for the same result).

## Session source
- **Decision**: Parse `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`.
- **Rationale**: Claude Code already persists everything there. Each line is a JSON event; `type: "user"|"assistant"` lines carry `message.content` (string or array of blocks), `type: "ai-title"` carries `aiTitle`, user lines carry `cwd`.
- **Alternatives considered**: running our own session store (violates Principle II).

## Sending prompts
- **Decision**: `claude -p --resume <id> <prompt>` with `current_dir = session.cwd`, on a `std::thread`, then reload the file.
- **Rationale**: Uses Claude's own resume semantics; zero protocol work.
- **Alternatives considered**: `--output-format stream-json` for live streaming (later); embedding a PTY running interactive `claude` (much bigger).
- **Note**: `-p` runs non-interactively, so tool permissions follow the user's settings; tools that need approval will be denied. Open question whether to pass a permission mode.

## Diffs
- **Decision**: From `tool_use` blocks named `Edit` (`old_string`→`new_string`), `MultiEdit` (each edit), `Write` (empty→`content`), line-diffed with `similar`.
- **Rationale**: Always available, no git required, matches what Claude actually did.
- **Alternatives considered**: `toolUseResult.structuredPatch` (has real line numbers, but not always present); `git diff` of the cwd (mixes in the user's own changes).

## Prompt history
- **Decision**: Per-session list of user prompts whose content is plain text, excluding command/caveat wrappers (`<command-name>`, `<local-command-…>`) and tool results.
- **Alternatives considered**: global history across sessions (easy to add later).

## Open questions (to settle by using it)
- Global vs per-session history?
- Group sessions by project in the sidebar?
- Live streaming of replies?
- Click a diff to jump to the message that made it?
- Auto-refresh by watching the file vs manual refresh?
