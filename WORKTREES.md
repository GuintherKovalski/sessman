# Making a change as an agent: worktrees and feature -> stage -> main

Several Claude sessions work in this repo at the same time, and the developer edits it too.
The main checkout (`~/Desktop/sessman`) is theirs. It is usually on `main`, often with
uncommitted work that isn't yours. So every agent makes its change in **its own git
worktree**, on its own `feature/*` branch, and never in the main checkout.

## Why

- `git switch`, `git stash` or `scripts/flow.sh new` in the main checkout would carry along,
  hide, or mix in someone else's uncommitted changes.
- Two agents on one checkout overwrite each other's files and builds.
- A worktree is a separate folder with its own branch and its own `target/`, sharing the
  same repository. What you do there doesn't touch any other checkout.

## The steps

```sh
# 1. From the main checkout, make a worktree on a new branch from origin/stage
cd ~/Desktop/sessman
git fetch origin
git worktree list                     # see what's there; pick a name nobody uses
git worktree add --no-track -b feature/<name> /tmp/scope-<name> origin/stage

# 2. Work only in the worktree: use absolute paths under /tmp/scope-<name>
cd /tmp/scope-<name>
# ...edit...
cargo test                            # first build in a new worktree takes a while (fresh target/)

# 3. Test in a separate instance, built from the worktree (see CLAUDE.md, "Testing changes")
scripts/test-instance.sh start <name>
export SCOPE_CONFIG=/tmp/scope-test-<name>/config
target/debug/scope list --workspace E2E
scripts/test-instance.sh stop <name>

# 4. Commit on the feature branch, then ship from the worktree
git add -A && git commit -m "..."
scripts/flow.sh ship                  # cargo test, push feature/<name>, merge into stage, push stage

# 5. Remove the worktree once it's on stage
cd ~/Desktop/sessman
git worktree remove /tmp/scope-<name>
```

The pipeline, each step and what does it:

1. **feature/<name>**: your commits, made from `origin/stage`. Pushed by `ship`.
2. **stage**: `ship` runs `cargo test`, merges your branch with `--no-ff`, and pushes `stage`.
   The pre-push hook takes only merges on `stage`, never direct commits.
3. **main**: CI (`.github/workflows/ci.yml`) tests `stage` again and fast-forwards `main` when
   the tests pass. Nobody pushes `main`. Check the run with `gh run watch`.

## Rules

- Never commit, switch branches, stash or reset in the main checkout. Don't touch files there
  that you didn't change; uncommitted changes there belong to someone else.
- One worktree per change, one `feature/*` branch per worktree. Name both after the change:
  `feature/<name>` in `/tmp/scope-<name>`.
- Always branch from `origin/stage` after a `git fetch`, not from `main` or a local `stage`.
- `ship` does `git switch stage` inside your worktree for a moment. Git lets a branch be
  checked out in only one worktree at a time, so **never leave a worktree on `stage`** (or
  `main`). If `ship` fails with "stage is already checked out at ...", that worktree has to
  switch away first; don't force it.
- If `ship` stops on a merge conflict, another change reached `stage` first. Leave `stage`
  alone (`git merge --abort; git switch feature/<name>`), merge `origin/stage` into your
  feature branch, fix it, `cargo test`, commit, and ship again.
- If `ship`'s `cargo test` fails, nothing has been merged: fix it on the feature branch.
- Commit and ship only when the developer asked for the change to go in. Tell them when it's
  on `stage`, and that they see it in their own scope after they restart it.
- Clean up: `git worktree remove` once the branch is shipped (or abandoned), and
  `scripts/test-instance.sh stop` for every instance you started. `git worktree prune` clears
  the entries of worktree folders that were deleted by hand.
