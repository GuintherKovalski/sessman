#!/bin/sh
# Builds scope.app (release binary + icon + Info.plist) and installs it.
#
#   scripts/bundle.sh [install dir]     default ~/Applications; "-" builds without installing
#
# Environment (set by scripts/release.sh; all optional):
#   SCOPE_UNIVERSAL=1        one binary for Apple Silicon and Intel (needs both Rust targets)
#   SCOPE_SIGN_IDENTITY=...  "Developer ID Application: …" signs with the hardened runtime,
#                            as notarization requires; unset = ad-hoc (fine on this Mac only)
#   SCOPE_RELEASE_REPO=o/r   GitHub repository the app looks in for updates
#
# The app is left in target/release/scope.app either way.
set -eu

cd "$(dirname "$0")/.."
dest="${1:-$HOME/Applications}"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

if [ "${SCOPE_UNIVERSAL:-}" = 1 ]; then
    for t in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --release --locked --target "$t"
    done
    mkdir -p target/release
    lipo -create -output target/release/scope-universal \
        target/aarch64-apple-darwin/release/scope target/x86_64-apple-darwin/release/scope
    bin=target/release/scope-universal
else
    cargo build --release
    bin=target/release/scope
fi

app="target/release/scope.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/scope"

# The .icns, from assets/icon.png (the same image the running app puts in the Dock).
iconset="target/release/scope.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size assets/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double assets/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/scope.icns"
rm -rf "$iconset"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>scope</string>
    <key>CFBundleDisplayName</key><string>scope</string>
    <key>CFBundleIdentifier</key><string>com.scope.app</string>
    <key>CFBundleExecutable</key><string>scope</string>
    <key>CFBundleIconFile</key><string>scope</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>scope $version</string>
    <key>NSMicrophoneUsageDescription</key><string>scope transcribes your dictation locally with Whisper.</string>
</dict>
</plist>
PLIST

if [ -n "${SCOPE_SIGN_IDENTITY:-}" ]; then
    # Hardened runtime + a secure timestamp: what notarization asks for.
    codesign --force --options runtime --timestamp \
        --entitlements packaging/scope.entitlements \
        --sign "$SCOPE_SIGN_IDENTITY" "$app"
    codesign --verify --deep --strict "$app"
else
    # Ad-hoc signature, so macOS keeps the microphone permission between launches.
    codesign --force --sign - "$app" >/dev/null 2>&1 || true
fi

if [ "$dest" = - ]; then
    echo "Built $app"
    exit 0
fi
mkdir -p "$dest"
rm -rf "$dest/scope.app"
cp -R "$app" "$dest/"
echo "Installed $dest/scope.app"
