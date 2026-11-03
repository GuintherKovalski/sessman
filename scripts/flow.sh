#!/bin/sh
# The road every change takes: feature/* -> stage -> tests pass -> main.
#
#   scripts/flow.sh new <name>   fetch, then start feature/<name> from origin/stage
#   scripts/flow.sh ship         test, merge the current feature branch into stage, push stage;
#                                CI tests stage again and fast-forwards main when it passes
#   scripts/flow.sh release <v>  bump the version to <v> and ship it; when CI promotes it to main,
#                                it builds, signs and publishes scope <v> (.github/workflows/release.yml)
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
release)
    version="${2:?usage: $0 release <version, e.g. 0.2.0>}"
    version="${version#v}"
    echo "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { echo "not a version: $version (want e.g. 0.2.0)" >&2; exit 1; }
    if [ -n "$(git status --porcelain)" ]; then echo "commit or stash your changes first" >&2; exit 1; fi
    git fetch origin
    current=$(git show origin/stage:Cargo.toml | sed -n 's/^version = "\(.*\)"/\1/p' | head -n 1)
    [ "$current" != "$version" ] || { echo "stage is already at $version" >&2; exit 1; }
    git switch --no-track -c "feature/release-$version" origin/stage
    sed -i.bak "1,/^version = /s/^version = \".*\"/version = \"$version\"/" Cargo.toml && rm Cargo.toml.bak
    cargo update --offline --workspace >/dev/null 2>&1 || cargo update --workspace
    git commit -am "Release $version"
    "$0" ship
    echo "Once CI promotes it to main, the release workflow publishes $version: gh run watch" ;;
setup)
    # Copied, not linked: a checkout of a branch without .githooks would take them away.
    git config --unset core.hooksPath || true
    hooks=$(git rev-parse --git-path hooks)
    mkdir -p "$hooks"
    cp .githooks/* "$hooks/"
    echo "hooks on: $(ls .githooks | tr '\n' ' ')(rerun after .githooks changes)" ;;
*)
    sed -n '2,10p' "$0" >&2
    exit 1 ;;
esac
