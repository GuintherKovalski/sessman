# Research: Session Search, Live Progress, Multi-line Prompt

## Live progress
- **Decision**: `claude -p --output-format stream-json --verbose --include-partial-messages`, applied to the open session in memory.
- **Rationale**: Tried locally: emits `stream_event` (`message_start`, `content_block_start`, `content_block_delta` with `text_delta`), complete `assistant` lines shaped like the session file, then a `result` line. Reusing the session parser for those lines gives text, tool calls, diffs and token counts mid-run with no new parsing.
- **Alternatives**: Poll the session file while running (no token-level text, and depends on when Claude Code flushes); `--output-format json` (only at the end).

## Search
- **Decision**: In-memory index built by a background thread (re-reads only changed files every 10 s); substring / all-words / small edit distance for names and goals; all-words substring for content.
- **Rationale**: No dependency, keeps typing instant (one pass over ≤ 200 kB per session only when the query changes), and replaces the ⟳ button with automatic refresh in the same pass.
- **Alternatives**: A fuzzy-matching crate (`nucleo`, `fuzzy-matcher`): better scoring but subsequence matching gives noisy hits on long titles; an on-disk index (overkill).

## AI search
- **Decision**: Haiku via `claude -p --model haiku --no-session-persistence`, given a numbered catalog (name, goal, first 3 prompts) of the 40 most recent sessions; replies `N: reason` lines.
- **Rationale**: Same call pattern as Summarize goal; a numbered list keeps the reply easy to parse and to validate.
