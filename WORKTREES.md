# Several features at once: one worktree, one build, one test instance each

You can have several changes to scope going at once, by hand or by agents, and build and try
each one on its own, while the scope you work in keeps running untouched. Each feature gets:

| what            | where                                       | why                                          |
|-----------------|---------------------------------------------|----------------------------------------------|
| a branch        | `feature/<name>`, made from `origin/stage`  | the road every change takes (CLAUDE.md)      |
| a folder        | `../sessman-features/<name>` (a git worktree) | its files and edits touch no other feature  |
| a build         | `target/` in that folder                    | builds never overwrite each other            |
| a test instance | `/tmp/scope-test-<name>`, its own window    | try it by hand without risking your sessions |

`scripts/feature.sh` does all of it:

```sh
scripts/feature.sh new <name>     # worktree + branch from origin/stage, build cache cloned in
scripts/feature.sh list           # every feature: commits vs stage, uncommitted files, built, running
scripts/feature.sh run <name>     # build it and open its own scope window to test by hand
scripts/feature.sh stop <name>    # close that window; deletes its config and the sessions made in it
scripts/feature.sh path <name>    # its folder:  cd "$(scripts/feature.sh path <name>)"
scripts/feature.sh build <name>   # just cargo build
scripts/feature.sh ship <name>    # cargo test, merge into stage, push (CI then moves main)
scripts/feature.sh rm <name>      # stop + remove the worktree; the branch too once it's on stage
```

It runs from any checkout (it finds the main one through git), and `<name>` is the part after
`feature/`. It works on every worktree on a `feature/*` branch, including older ones made by
hand in `/tmp` or on the Desktop.

## The layout

```
~/Desktop/sessman/                  main checkout: yours, usually on main, never built on by agents
~/Desktop/sessman-features/
    dark-mode/                      feature/dark-mode   (own target/, own edits)
    faster-search/                  feature/faster-search
/tmp/scope-test-dark-mode/          its test instance: config/, proj/, app.log, pid
/tmp/scope-test-faster-search/
```

Set `SCOPE_FEATURES` to put the worktrees somewhere else. They are kept out of `/tmp` on
purpose: macOS deletes files in `/tmp` that haven't been opened for 3 days, and a worktree
holds uncommitted work. Test instances are throwaway, so they stay in `/tmp`.

## A day with three features

```sh
cd ~/Desktop/sessman
scripts/feature.sh new dark-mode
scripts/feature.sh new faster-search
scripts/feature.sh new tab-icons

# Work in each folder: an editor window per folder, or a scope session per feature whose
# folder is the worktree (scope new --folder ~/Desktop/sessman-features/dark-mode "...").

scripts/feature.sh run dark-mode        # a scope window built from dark-mode
scripts/feature.sh run faster-search    # another one, side by side; each has its own config
scripts/feature.sh list

# Changed dark-mode again? Rebuild and reopen just that one:
scripts/feature.sh stop dark-mode && scripts/feature.sh run dark-mode

# Happy with it:
cd "$(scripts/feature.sh path dark-mode)"
git add -A && git commit -m "Dark mode: ..."
scripts/feature.sh ship dark-mode
scripts/feature.sh rm dark-mode         # once ship says it's on stage
```

## Builds

The first build in a new worktree would compile every dependency again (minutes, ~3 GB).
`new` starts the worktree's `target/` as an APFS clone of the main checkout's: no extra disk
until files change, and the first `cargo build` only compiles scope itself (seconds). If the
main checkout has never been built, or the disk isn't APFS, it starts empty and the first
build is a full one.

`target/` folders do grow as features diverge; `rm` deletes the folder and its build with it.
`du -sh ~/Desktop/sessman-features/*/target` shows what they take.

## Trying a feature by hand

`run` is `scripts/test-instance.sh start <name>` from the feature's folder: it builds, copies
your config (workspaces, settings, not what's in flight) into `/tmp/scope-test-<name>`, and
opens a scope window on an **E2E** workspace whose new sessions start in
`/tmp/scope-test-<name>/proj`. What you do in that window (renames, moves, settings, new
sessions) stays in its copy.

- The windows all look like scope. Tell them apart by the order you opened them, or close
  the ones you're not testing.
- Every instance shows all your sessions as working (it finds them by their processes). Only
  use the E2E workspace there; don't stop or prompt your real sessions from a test window.
- Drive an instance from the terminal too:
  `SCOPE_CONFIG=/tmp/scope-test-<name>/config "$(scripts/feature.sh path <name>)"/target/debug/scope list --workspace E2E`
- Something wrong? `/tmp/scope-test-<name>/app.log`.
- `stop` deletes the instance and the Claude sessions made in it. To see a feature in your
  own scope, ship it and restart your scope once it's on main.

## Rules (agents and people)

- Never commit, switch branches, stash or reset in the main checkout. Its uncommitted
  changes belong to someone else. Make every change in a worktree.
- One feature = one worktree = one `feature/*` branch = one test instance, all with the same
  name. Check `scripts/feature.sh list` before choosing a name.
- Always start from `origin/stage` (`new` does). If a branch `feature/<name>` already exists,
  `new` checks it out instead of making a fresh one.
- `ship` does `git switch stage` inside the worktree for a moment. Git checks a branch out in
  only one worktree at a time, so **never leave a worktree on `stage`** (or `main`). If `ship`
  says "stage is already checked out at ...", that worktree has to switch away first.
- If `ship` stops on a merge conflict, another change reached stage first. Leave stage
  alone (`git merge --abort; git switch feature/<name>`), merge `origin/stage` into the
  feature branch, fix it, `cargo test`, commit, and ship again. If its `cargo test` fails,
  nothing was merged: fix it on the branch.
- A feature that's behind stage (`list` shows `-N`): `git merge origin/stage` in its folder to
  test it against the latest.
- Agents: commit and ship only when the developer asked. Say when it's on stage, and that
  they see it in their own scope after a restart.
- Clean up: `rm` once a feature is shipped or abandoned (it keeps a branch that isn't on
  stage), `stop` every instance you started. `git worktree prune` clears entries of
  worktree folders deleted by hand.

## Without the script

```sh
git fetch origin
git worktree add --no-track -b feature/<name> ~/Desktop/sessman-features/<name> origin/stage
cp -cR ~/Desktop/sessman/target ~/Desktop/sessman-features/<name>/target   # optional build cache
cd ~/Desktop/sessman-features/<name>
scripts/test-instance.sh start <name>      # ... test ...
scripts/test-instance.sh stop <name>
scripts/flow.sh ship
cd ~/Desktop/sessman && git worktree remove ~/Desktop/sessman-features/<name>
```
