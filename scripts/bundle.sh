#!/bin/sh
# Builds scope.app (release binary + icon) so the Dock and Finder show the app icon even when
# scope is not running. Usage: scripts/bundle.sh [install dir, default ~/Applications]
set -eu

cd "$(dirname "$0")/.."
dest="${1:-$HOME/Applications}"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

cargo build --release

app="target/release/scope.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/scope "$app/Contents/MacOS/scope"

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

cat > "$app/Contents/Info.plist" <<EOF
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
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSMicrophoneUsageDescription</key><string>scope transcribes your dictation locally with Whisper.</string>
</dict>
</plist>
EOF

# Ad-hoc signature, so macOS keeps the microphone permission between launches.
codesign --force --sign - "$app" >/dev/null 2>&1 || true

mkdir -p "$dest"
rm -rf "$dest/scope.app"
cp -R "$app" "$dest/"
echo "Installed $dest/scope.app"
