# Quickstart: validating 011

```sh
cargo test            # session apply, scoring, ranking, snippets, AI reply parsing
cargo run --release
```

1. Left panel: no "Sessions ⟳" header; theme switch sits next to Changes / Info. Start a session in a terminal → it appears within ~10 s.
2. Type part of a session name in the bottom search → it's listed first. Type a misspelling ("rendring") → still found. Type a word only in a conversation → listed with a snippet. Escape → normal list.
3. Type a description ("where we added workspace colors") and press Enter → spinner, then sessions with reasons.
4. Send a prompt that reads and edits files → prompt shows at once, text streams in, `⚙ Read` / `⚙ Edit` lines appear, Changes tab updates before the run ends.
5. In the prompt input: Shift+Enter adds lines and the box grows; past ~10 lines it scrolls; Enter sends. Up/Down on a one-line input still walks history.
