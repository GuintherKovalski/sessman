# sessman: notes for agents

sessman is the IDE you are probably running in. Read [PROJECT.md](PROJECT.md) for how it is
built and [ROADMAP.md](ROADMAP.md) for what comes next.

## Testing changes: always in a new instance, never the running one

The Claude sessions sessman runs, this one most likely included, are child processes of the
running sessman. If you quit, restart, `kill` or reinstall it (`scripts/bundle.sh` over
`~/Applications/sessman.app` while it runs), the sessions it runs end, and you end with them.
Until sessman can take changes live (ROADMAP item 13), test every change in a separate
instance, one per test:

```sh
scripts/test-instance.sh start [name]   # builds, then runs target/debug/sessman with a copy of the config
export SESSMAN_CONFIG=/tmp/sessman-test-<name>/config
target/debug/sessman list --workspace E2E        # drive it through its commands
scripts/test-instance.sh stop [name]    # quits it; removes its folder and the sessions made in it
```

- The instance has its own config (`SESSMAN_CONFIG`), so renames, moves, workspaces and state
  you change there never reach the developer's. Commands (`sessman list|rename|move|new|ask`)
  reach the instance whose `SESSMAN_CONFIG` they run with.
- It opens on an **E2E** workspace whose new sessions start in `/tmp/sessman-test-<name>/proj`.
  Sessions made there are real Claude Code sessions; `stop` deletes them.
- Check results through what the app itself reports: command replies, `sessman list` (the
  bar's order), and the files it writes under the instance's config. Screenshots of its window
  usually don't work (no screen-recording permission); it keeps working while hidden.
- A new session shows in `sessman list` as `[starting]` until sessman has indexed it, a moment
  after its first run ends; wait for that mark to go before reading it as a normal session.
- Keep prompts in test sessions trivial ("Reply with exactly: X ready.") to stay fast and cheap.
- Never touch `~/.config/sessman` or the running sessman to test. When the developer should see
  a change in their own sessman, say so: they restart it themselves.

`cargo test` covers the logic and, in `src/e2e`, the whole app driven headless against a fake
`claude` (no real sessions, no cost); run it before any instance.
