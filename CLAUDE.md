# scope: notes for agents

scope is the IDE you are probably running in. Read [PROJECT.md](PROJECT.md) for how it is
built and [ROADMAP.md](ROADMAP.md) for what comes next.

## Branches: feature -> stage -> tests pass -> main

Never commit on `main` or `stage`, and never work in the main checkout: it holds the
developer's (and other agents') uncommitted changes. Make each change in its own git worktree;
[WORKTREES.md](WORKTREES.md) has the steps. Every change goes:

```sh
scripts/flow.sh new <name>   # feature/<name>, made from origin/stage
# ...commit there...
scripts/flow.sh ship         # cargo test, merge into stage, push stage
```

CI (`.github/workflows/ci.yml`) runs the tests on every push to stage and, when they pass,
fast-forwards `main` to it. Nobody pushes `main`. The hooks in `.githooks` refuse commits on
main/stage, pushes to main, direct commits pushed to stage, and pushing branches not named
`feature/*`; a fresh clone installs them with `scripts/flow.sh setup`. (The repo is private on
a free plan, so GitHub can't protect the branches itself; CI flags a push to main that skipped
stage, and refuses pull requests that don't go feature/* -> stage.)

## Testing changes: always in a new instance, never the running one

The Claude sessions scope runs, this one most likely included, are child processes of the
running scope. Quitting it doesn't end them (the next scope to start picks them up), but the
developer loses the window they work in, and a `kill` of its process group or a crash in the
middle of a test does end them. Until scope can take changes live (ROADMAP item 13), test
every change in a separate instance, one per test:

```sh
scripts/test-instance.sh start [name]   # builds, then runs target/debug/scope with a copy of the config
export SCOPE_CONFIG=/tmp/scope-test-<name>/config
target/debug/scope list --workspace E2E        # drive it through its commands
scripts/test-instance.sh stop [name]    # quits it; removes its folder and the sessions made in it
```

- The instance has its own config (`SCOPE_CONFIG`), so renames, moves, workspaces and state
  you change there never reach the developer's. Commands (`scope list|rename|move|new|ask`)
  reach the instance whose `SCOPE_CONFIG` they run with.
- It opens on an **E2E** workspace whose new sessions start in `/tmp/scope-test-<name>/proj`.
  Sessions made there are real Claude Code sessions; `stop` deletes them.
- Check results through what the app itself reports: command replies, `scope list` (the
  bar's order), and the files it writes under the instance's config. Screenshots of its window
  usually don't work (no screen-recording permission); it keeps working while hidden.
- A new session shows in `scope list` as `[starting]` until scope has indexed it, a moment
  after its first run ends; wait for that mark to go before reading it as a normal session.
- An instance shows every session's runs, the developer's own included, as working (it finds
  them by their processes). Never stop one there, and don't send prompts to sessions outside
  its E2E workspace.
- Keep prompts in test sessions trivial ("Reply with exactly: X ready.") to stay fast and cheap.
- Never touch `~/.config/scope` or the running scope to test. When the developer should see
  a change in their own scope, say so: they restart it themselves.

`cargo test` covers the logic and, in `src/e2e`, the whole app driven headless against a fake
`claude` (no real sessions, no cost); run it before any instance.
