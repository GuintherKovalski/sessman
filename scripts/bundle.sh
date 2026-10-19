#!/bin/sh
# Builds sessman.app (release binary + icon) so the Dock and Finder show the app icon even when
# sessman is not running. Usage: scripts/bundle.sh [install dir, default ~/Applications]
set -eu

cd "$(dirname "$0")/.."
dest="${1:-$HOME/Applications}"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

cargo build --release

app="target/release/sessman.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/sessman "$app/Contents/MacOS/sessman"

# The .icns, from assets/icon.png (the same image the running app puts in the Dock).
iconset="target/release/sessman.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size assets/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double assets/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/sessman.icns"
rm -rf "$iconset"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>sessman</string>
    <key>CFBundleDisplayName</key><string>sessman</string>
    <key>CFBundleIdentifier</key><string>com.sessman.app</string>
    <key>CFBundleExecutable</key><string>sessman</string>
    <key>CFBundleIconFile</key><string>sessman</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSMicrophoneUsageDescription</key><string>sessman transcribes your dictation locally with Whisper.</string>
</dict>
</plist>
EOF

# Ad-hoc signature, so macOS keeps the microphone permission between launches.
codesign --force --sign - "$app" >/dev/null 2>&1 || true

mkdir -p "$dest"
rm -rf "$dest/sessman.app"
cp -R "$app" "$dest/"
echo "Installed $dest/sessman.app"
