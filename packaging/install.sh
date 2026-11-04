#!/bin/sh
# Installs or updates scope, the IDE for Claude Code sessions, from its latest release:
#
#   curl -fsSL https://github.com/GuintherKovalski/sessman/releases/latest/download/install.sh | sh
#
# Puts scope.app in /Applications (or ~/Applications when that isn't writable) and opens it.
# Environment, all optional:
#   SCOPE_REPO=owner/name   repository to install from
#   SCOPE_DIR=/path         where to put scope.app
#   SCOPE_FROM=URL          folder holding scope-macos.zip and SHA256SUMS (default: latest release)
#   SCOPE_NO_OPEN=1         don't open it afterwards
#   SCOPE_NO_MODEL=1        don't download the speech model for dictation (about 150 MB)
#   SCOPE_NO_CLAUDE=1       don't install Claude Code when it is missing
set -eu

repo="${SCOPE_REPO:-GuintherKovalski/sessman}"
from="${SCOPE_FROM:-https://github.com/$repo/releases/latest/download}"

say() { printf '%s\n' "$*"; }
fail() { printf 'scope: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail "scope runs on macOS only for now."
major=$(sw_vers -productVersion | cut -d. -f1)
[ "$major" -ge 11 ] || fail "scope needs macOS 11 or later (this is $(sw_vers -productVersion))."

if [ -n "${SCOPE_DIR:-}" ]; then
    dest="$SCOPE_DIR"
elif [ -w /Applications ]; then
    dest=/Applications
else
    dest="$HOME/Applications"
fi
mkdir -p "$dest"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading scope…"
curl -fL --progress-bar -o "$tmp/scope-macos.zip" "$from/scope-macos.zip" \
    || fail "couldn't download $from/scope-macos.zip"
curl -fsSL -o "$tmp/SHA256SUMS" "$from/SHA256SUMS" || fail "couldn't download the checksums"
want=$(awk '$2 == "scope-macos.zip" { print $1 }' "$tmp/SHA256SUMS")
have=$(shasum -a 256 "$tmp/scope-macos.zip" | cut -d' ' -f1)
[ -n "$want" ] && [ "$want" = "$have" ] || fail "the download doesn't match its checksum; nothing was installed."

ditto -x -k "$tmp/scope-macos.zip" "$tmp/unpacked"
new="$tmp/unpacked/scope.app"
[ -x "$new/Contents/MacOS/scope" ] || fail "the download has no scope.app in it."
codesign --verify --deep --strict "$new" 2>/dev/null || fail "the app's signature doesn't check out; nothing was installed."
version=$(defaults read "$new/Contents/Info.plist" CFBundleShortVersionString 2>/dev/null || echo "")

app="$dest/scope.app"
running=no
pgrep -qf "$app/Contents/MacOS/scope" && running=yes
# Swapped in one move, so a failure never leaves half an app.
rm -rf "$dest/.scope-old"
[ -e "$app" ] && mv "$app" "$dest/.scope-old"
if ! ditto "$new" "$app"; then
    [ -e "$dest/.scope-old" ] && mv "$dest/.scope-old" "$app"
    fail "couldn't write $app"
fi
rm -rf "$dest/.scope-old"
say "Installed scope ${version:+$version }in $app"

# Dictation's speech model, so the mic works at once (scope would otherwise ask the first time).
model_dir="${SCOPE_CONFIG:-$HOME/.config/scope}/models"
model="$model_dir/ggml-base.bin"
model_bytes=147951465
if [ -z "${SCOPE_NO_MODEL:-}" ] && [ "$(stat -f %z "$model" 2>/dev/null || echo 0)" != "$model_bytes" ]; then
    say "Downloading the speech model for dictation (about 150 MB)…"
    mkdir -p "$model_dir"
    if curl -fL --progress-bar -o "$model.part" "${SCOPE_MODEL_URL:-https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin}" \
        && [ "$(stat -f %z "$model.part")" = "$model_bytes" ]; then
        mv "$model.part" "$model"
    else
        rm -f "$model.part"
        say "Couldn't download the speech model; scope offers it again the first time you dictate."
    fi
fi

# Claude Code, which scope runs the sessions with.
has_claude() { command -v claude >/dev/null 2>&1 || [ -x "$HOME/.local/bin/claude" ] || [ -x "$HOME/.claude/local/claude" ]; }
if ! has_claude; then
    if [ -z "${SCOPE_NO_CLAUDE:-}" ]; then
        say "Installing Claude Code (scope runs your sessions with it)…"
        curl -fsSL https://claude.ai/install.sh | bash || true
    fi
    if ! has_claude; then
        say ""
        say "scope needs Claude Code. Install it with:  curl -fsSL https://claude.ai/install.sh | bash"
    fi
fi
if has_claude; then
    say "Claude Code is ready. If you haven't yet, sign in once by running: claude"
fi

if [ "$running" = yes ]; then
    say "scope is open: quit it (Cmd+Q) and open it again to use the new version. Your sessions keep running."
elif [ -z "${SCOPE_NO_OPEN:-}" ]; then
    open "$app"
fi
