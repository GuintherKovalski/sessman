# Quickstart: validating 010

```sh
cargo test            # block splitting: tables, ragged rows, tool lines, fences
cargo run --release
```

1. Open a session where Claude replied with a table → it shows as a bordered grid, bold header, aligned columns; a wide one scrolls sideways.
2. Same session: code blocks are highlighted; headings, lists, bold, links render; clicking a link opens the browser. `[tool: X]` lines are small and dimmed.
3. Scroll a long session (300+ messages) → as smooth as before.
4. Changes tab → double-click a `.md` file → it opens rendered; **Source** shows line numbers; **Rendered** goes back.
5. Switch to Light → prompt input, diffs, cards, code all readable. Restart → still Light. Pick System, toggle macOS appearance → app follows.
