#!/bin/sh
# The road every change takes: feature/* -> stage -> tests pass -> main.
#
#   scripts/flow.sh new <name>   fetch, then start feature/<name> from origin/stage
#   scripts/flow.sh ship         test, merge the current feature branch into stage, push stage;
#                                CI tests stage again and fast-forwards main when it passes
#   scripts/flow.sh setup        install the hooks in .githooks that keep the road (once per clone)
set -eu

cd "$(dirname "$0")/.."
cmd="${1:-}"

case "$cmd" in
new)
    name="${2:?usage: $0 new <name>}"
    git fetch origin
    git switch --no-track -c "feature/${name#feature/}" origin/stage ;;
ship)
    branch=$(git symbolic-ref --short HEAD)
    case "$branch" in feature/*) ;; *) echo "not on a feature/* branch ($branch)" >&2; exit 1 ;; esac
    if [ -n "$(git status --porcelain)" ]; then echo "commit or stash your changes first" >&2; exit 1; fi
    cargo test
    git push -u origin "$branch"
    git fetch origin
    git switch stage
    git merge --ff-only origin/stage
    git merge --no-ff --no-edit "$branch"
    git push origin stage
    git switch "$branch"
    echo "$branch is on stage; CI promotes it to main once the tests pass: gh run watch" ;;
setup)
    # Copied, not linked: a checkout of a branch without .githooks would take them away.
    git config --unset core.hooksPath || true
    hooks=$(git rev-parse --git-path hooks)
    mkdir -p "$hooks"
    cp .githooks/* "$hooks/"
    echo "hooks on: $(ls .githooks | tr '\n' ' ')(rerun after .githooks changes)" ;;
*)
    sed -n '2,8p' "$0" >&2
    exit 1 ;;
esac
