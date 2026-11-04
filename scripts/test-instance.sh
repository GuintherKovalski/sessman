#!/bin/sh
# Runs a separate scope to test changes in, never the one you are working in: the Claude
# sessions scope runs are its child processes, so restarting it ends them (and the agent
# making the change with them).
#
#   scripts/test-instance.sh start [name] [label]
#                                           build, then launch an instance with its own copy of
#                                           ~/.config/scope; prints how to drive it
#   scripts/test-instance.sh stop [name]    quit it, and delete it and the sessions made in it
#
# The instance lives in /tmp/scope-test-<name> (default name: "default"):
#   config/  its config (SCOPE_CONFIG); changes there never reach your own
#   proj/    folder for test sessions, the default for its "E2E" workspace
#   pid, app.log
#   scope: <label>.app   the binary it runs, in a bundle named after what is being tested, so
#                        hovering its Dock icon tells the instances apart. The label defaults
#                        to the branch (without "feature/"), plus the name when not "default".
set -eu

cd "$(dirname "$0")/.."
cmd="${1:-}"
name="${2:-default}"
label="${3:-}"
dir="/tmp/scope-test-$name"
bin="$PWD/target/debug/scope"

case "$cmd" in
start)
    if [ -f "$dir/pid" ] && kill -0 "$(cat "$dir/pid")" 2>/dev/null; then
        echo "already running (pid $(cat "$dir/pid")); stop it first: $0 stop $name" >&2
        exit 1
    fi
    cargo build
    rm -rf "$dir"
    mkdir -p "$dir/config" "$dir/proj"
    # A copy of your config, without the Whisper models (linked) and anything in flight: the
    # one the scope you work in uses ($SCOPE_CONFIG, inherited by its sessions), unless that
    # is this instance's own. (~/.config/sessman until your scope has run since the rename.)
    own="${SCOPE_CONFIG:-$HOME/.config/scope}"
    case "$own" in "$dir"/*|"$dir") own="$HOME/.config/scope" ;; esac
    [ -d "$own" ] || own="$HOME/.config/sessman"
    for f in "$own"/*; do
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
    # The Dock names a running app after the bundle its binary is in (CFBundleName).
    if [ -z "$label" ]; then
        label=$(git symbolic-ref --short HEAD 2>/dev/null || git rev-parse --short HEAD)
        label="${label#feature/}"
        [ "$name" = default ] || [ "$name" = "$label" ] || label="$label ($name)"
    fi
    label="scope: $(printf '%s' "$label" | tr '/' '-')"
    app="$dir/$label.app"
    mkdir -p "$app/Contents/MacOS"
    cp -c "$bin" "$app/Contents/MacOS/scope" 2>/dev/null || cp "$bin" "$app/Contents/MacOS/scope"
    xml=$(printf '%s' "$label" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')
    id=$(printf '%s' "$name" | tr -c 'A-Za-z0-9.' '-')
    cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$xml</string>
    <key>CFBundleDisplayName</key><string>$xml</string>
    <key>CFBundleIdentifier</key><string>com.scope.test.$id</string>
    <key>CFBundleExecutable</key><string>scope</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSMicrophoneUsageDescription</key><string>scope transcribes your dictation locally with Whisper.</string>
</dict>
</plist>
EOF
    codesign --force --sign - "$app" >/dev/null 2>&1 || true
    SCOPE_CONFIG="$dir/config" nohup "$app/Contents/MacOS/scope" >"$dir/app.log" 2>&1 &
    echo $! >"$dir/pid"
    sleep 3
    if ! kill -0 "$(cat "$dir/pid")" 2>/dev/null; then
        echo "it exited at once; see $dir/app.log" >&2
        exit 1
    fi
    cat <<EOF
Test instance "$name" running (pid $(cat "$dir/pid")), on its E2E workspace.
Its Dock icon reads "$label".
Drive it with its own config:
  export SCOPE_CONFIG=$dir/config
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
    project="$HOME/.claude/projects/$(echo "$(cd /tmp && pwd -P)/scope-test-$name/proj" | sed 's/[^A-Za-z0-9]/-/g')"
    rm -rf "$project" "$dir"
    echo "stopped and removed test instance \"$name\""
    ;;
*)
    sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
