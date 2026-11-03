#!/bin/sh
# Installs (or updates) scope from its latest release:
#   curl -fsSL https://github.com/REPO/releases/latest/download/install.sh | sh
# Puts scope.app in /Applications (or ~/Applications when that isn't writable) and opens it.
set -eu

repo="${SCOPE_REPO:-REPO}"
[ "$(uname -s)" = Darwin ] || { echo "scope runs on macOS only for now." >&2; exit 1; }

dest=/Applications
[ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }

tag=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)
[ -n "$tag" ] || { echo "Couldn't find a release of $repo." >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading scope $tag…"
curl -fL --progress-bar -o "$tmp/scope.zip" "https://github.com/$repo/releases/download/v$tag/scope-$tag-macos.zip"
curl -fsSL -o "$tmp/SHA256SUMS" "https://github.com/$repo/releases/download/v$tag/SHA256SUMS"
want=$(grep "scope-$tag-macos.zip" "$tmp/SHA256SUMS" | cut -d' ' -f1)
have=$(shasum -a 256 "$tmp/scope.zip" | cut -d' ' -f1)
[ "$want" = "$have" ] || { echo "Checksum mismatch; not installing." >&2; exit 1; }

ditto -x -k "$tmp/scope.zip" "$tmp"
codesign --verify --deep --strict "$tmp/scope.app"
rm -rf "$dest/scope.app"
ditto "$tmp/scope.app" "$dest/scope.app"
echo "Installed scope $tag in $dest/scope.app"

if ! command -v claude >/dev/null 2>&1 && [ ! -x "$HOME/.local/bin/claude" ] && [ ! -x "$HOME/.claude/local/claude" ]; then
    echo
    echo "scope runs Claude Code sessions, so it needs Claude Code itself:"
    echo "  curl -fsSL https://claude.ai/install.sh | bash"
fi
open "$dest/scope.app"
