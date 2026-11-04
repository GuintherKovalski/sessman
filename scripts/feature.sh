#!/bin/sh
# Several changes at once: each feature gets its own worktree (folder + feature/<name> branch),
# its own build (target/ inside that folder) and its own test instance of scope.
#
#   scripts/feature.sh new <name>     worktree on feature/<name> from origin/stage, in
#                                     $SCOPE_FEATURES/<name> (default ../sessman-features/<name>)
#   scripts/feature.sh list           every worktree: branch, commits vs stage, uncommitted
#                                     files, build time, test instance running or not
#   scripts/feature.sh path <name>    print its folder:  cd "$(scripts/feature.sh path <name>)"
#   scripts/feature.sh build <name>   cargo build there
#   scripts/feature.sh run <name>     build it and open its own test instance (a scope window
#                                     to try by hand), config in /tmp/scope-test-<name>
#   scripts/feature.sh stop <name>    quit that instance; delete it and the sessions made in it
#   scripts/feature.sh ship <name>    scripts/flow.sh ship from it (test, merge into stage)
#   scripts/feature.sh rm <name>      stop its instance and remove the worktree (refused while it
#                                     has uncommitted changes); the branch goes too once it is on
#                                     origin/stage
#
# <name> is the part after feature/; any worktree on a feature/* branch works, wherever it is.
set -eu

# The main checkout, whichever worktree this runs from.
main=$(cd "$(dirname "$0")/.." && cd "$(git rev-parse --git-common-dir)/.." && pwd -P)
root="${SCOPE_FEATURES:-$(dirname "$main")/sessman-features}"
cmd="${1:-}"

usage() { sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }

# The folder of the worktree on feature/<name>, or nothing.
path_of() {
    git -C "$main" worktree list --porcelain | awk -v b="refs/heads/feature/$1" '
        /^worktree / { p = substr($0, 10) }
        $0 == "branch " b { print p; exit }'
}

need() {
    p=$(path_of "$1")
    [ -n "$p" ] || { echo "no worktree on feature/$1 (scripts/feature.sh list)" >&2; exit 1; }
    echo "$p"
}

running() { [ -f "/tmp/scope-test-$1/pid" ] && kill -0 "$(cat "/tmp/scope-test-$1/pid")" 2>/dev/null; }

case "$cmd" in
new)
    name="${2:?usage: $0 new <name>}"; name="${name#feature/}"
    dir="$root/$name"
    [ -z "$(path_of "$name")" ] || { echo "feature/$name already has a worktree: $(path_of "$name")" >&2; exit 1; }
    [ ! -e "$dir" ] || { echo "$dir already exists" >&2; exit 1; }
    git -C "$main" fetch -q origin
    if git -C "$main" rev-parse -q --verify "refs/heads/feature/$name" >/dev/null; then
        git -C "$main" worktree add "$dir" "feature/$name"          # an existing branch: pick it up
    else
        git -C "$main" worktree add --no-track -b "feature/$name" "$dir" origin/stage
    fi
    # Start its build from the main checkout's: an APFS clone (no extra disk until files
    # change), so only scope itself compiles, not every dependency again.
    if [ -d "$main/target" ] && cp -cR "$main/target" "$dir/target" 2>/dev/null; then
        echo "target/ cloned from $main (copy-on-write)"
    else
        rm -rf "$dir/target"
        echo "target/ starts empty: the first build compiles everything"
    fi
    cat <<EOF

feature/$name is in $dir
  work (and run agents) in:  cd $dir
  build + try it by hand:    scripts/feature.sh run $name
EOF
    ;;
list)
    git -C "$main" fetch -q origin 2>/dev/null || true
    printf '%-24s %-9s %-7s %-14s %-9s %s\n' FEATURE "VS STAGE" DIRTY BUILT INSTANCE FOLDER
    git -C "$main" worktree list --porcelain | awk '
        /^worktree / { p = substr($0, 10) }
        /^branch /   { b = substr($0, 19) }
        /^detached/  { b = "(detached)" }
        /^$/         { if (p != "") print b "\t" p; p = ""; b = "" }
        END          { if (p != "") print b "\t" p }' |
    while IFS="$(printf '\t')" read -r branch p; do
        name="${branch#feature/}"
        if [ -d "$p" ]; then
            vs=$(git -C "$p" rev-list --left-right --count origin/stage...HEAD 2>/dev/null |
                awk '{ printf "+%s -%s", $2, $1 }')
            dirty=$(git -C "$p" status --porcelain 2>/dev/null | wc -l | tr -d ' ')
        else
            vs="?"; dirty="gone"
        fi
        built=$( [ -f "$p/target/debug/scope" ] && date -r "$p/target/debug/scope" '+%b %d %H:%M' || echo -)
        inst=-
        case "$branch" in feature/*) running "$name" && inst="running" ;; esac
        printf '%-24s %-9s %-7s %-14s %-9s %s\n' "$name" "$vs" "$dirty" "$built" "$inst" "$p"
    done
    echo "(VS STAGE: commits ahead +, behind - of origin/stage; DIRTY: uncommitted files)"
    ;;
path)
    need "${2:?usage: $0 path <name>}" ;;
build)
    p=$(need "${2:?usage: $0 build <name>}")
    cd "$p" && cargo build ;;
run)
    name="${2:?usage: $0 run <name>}"
    p=$(need "$name")
    cd "$p" && scripts/test-instance.sh start "$name" ;;
stop)
    name="${2:?usage: $0 stop <name>}"
    p=$(need "$name")
    cd "$p" && scripts/test-instance.sh stop "$name" ;;
ship)
    p=$(need "${2:?usage: $0 ship <name>}")
    cd "$p" && scripts/flow.sh ship ;;
rm)
    name="${2:?usage: $0 rm <name>}"
    p=$(need "$name")
    [ "$p" != "$main" ] || { echo "that's the main checkout; not removing it" >&2; exit 1; }
    if [ -n "$(git -C "$p" status --porcelain)" ]; then
        echo "feature/$name has uncommitted changes in $p; commit or discard them first" >&2
        exit 1
    fi
    if running "$name"; then (cd "$p" && scripts/test-instance.sh stop "$name"); fi
    git -C "$main" worktree remove "$p"
    git -C "$main" fetch -q origin
    if git -C "$main" merge-base --is-ancestor "feature/$name" origin/stage; then
        git -C "$main" branch -D "feature/$name" >/dev/null
        echo "removed $p and feature/$name (it is on stage)"
    else
        echo "removed $p; kept feature/$name (not on stage yet): scripts/feature.sh new $name picks it up again"
    fi
    ;;
*)
    usage ;;
esac
