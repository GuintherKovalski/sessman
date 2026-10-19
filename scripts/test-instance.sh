#!/bin/sh
# Runs a separate sessman to test changes in, never the one you are working in: the Claude
# sessions sessman runs are its child processes, so restarting it ends them (and the agent
# making the change with them).
#
#   scripts/test-instance.sh start [name]   build, then launch an instance with its own copy of
#                                           ~/.config/sessman; prints how to drive it
#   scripts/test-instance.sh stop [name]    quit it, and delete it and the sessions made in it
#
# The instance lives in /tmp/sessman-test-<name> (default name: "default"):
#   config/  its config (SESSMAN_CONFIG); changes there never reach your own
#   proj/    folder for test sessions, the default for its "E2E" workspace
#   pid, app.log
set -eu

cd "$(dirname "$0")/.."
cmd="${1:-}"
name="${2:-default}"
dir="/tmp/sessman-test-$name"
bin="$PWD/target/debug/sessman"

case "$cmd" in
start)
    if [ -f "$dir/pid" ] && kill -0 "$(cat "$dir/pid")" 2>/dev/null; then
        echo "already running (pid $(cat "$dir/pid")); stop it first: $0 stop $name" >&2
        exit 1
    fi
    cargo build
    rm -rf "$dir"
    mkdir -p "$dir/config" "$dir/proj"
    # A copy of your config, without the Whisper models (linked) and anything in flight.
    for f in "$HOME/.config/sessman"/*; do
        case "$(basename "$f")" in
        models) ln -s "$f" "$dir/config/models" ;;
        inbox) ;;
        *) cp -R "$f" "$dir/config/" ;;
        esac
    done
    # An "E2E" workspace whose new sessions start in proj/, open at start.
    python3 - "$dir" <<'EOF'
import json, os, sys
d = sys.argv[1]
p = os.path.join(d, "config", "workspaces.json")
w = json.load(open(p)) if os.path.exists(p) else {"list": []}
w["list"] = [x for x in w["list"] if x["name"] != "E2E"]
w["list"].append({"name": "E2E", "sessions": [], "color": [220, 190, 80], "last": "@orchestration", "folder": os.path.join(d, "proj")})
json.dump(w, open(p, "w"), indent=2)
s = os.path.join(d, "config", "state.json")
st = json.load(open(s)) if os.path.exists(s) else {}
st.update(tab="E2E", open=None)
json.dump(st, open(s, "w"))
EOF
    SESSMAN_CONFIG="$dir/config" nohup "$bin" >"$dir/app.log" 2>&1 &
    echo $! >"$dir/pid"
    sleep 3
    if ! kill -0 "$(cat "$dir/pid")" 2>/dev/null; then
        echo "it exited at once; see $dir/app.log" >&2
        exit 1
    fi
    cat <<EOF
Test instance "$name" running (pid $(cat "$dir/pid")), on its E2E workspace.
Drive it with its own config:
  export SESSMAN_CONFIG=$dir/config
  $bin list --workspace E2E
  $bin new --workspace E2E --name Alpha "Reply with exactly: Alpha ready."
Stop it and clean up:  $0 stop $name
EOF
    ;;
stop)
    if [ -f "$dir/pid" ]; then
        kill "$(cat "$dir/pid")" 2>/dev/null || true
    fi
    # Sessions made in it are real Claude Code sessions: remove them from ~/.claude/projects.
    project="$HOME/.claude/projects/$(echo "$(cd /tmp && pwd -P)/sessman-test-$name/proj" | sed 's/[^A-Za-z0-9]/-/g')"
    rm -rf "$project" "$dir"
    echo "stopped and removed test instance \"$name\""
    ;;
*)
    sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
