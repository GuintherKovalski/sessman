#!/bin/sh
# Builds the files a release publishes, into dist/:
#
#   scope-<version>.dmg          drag-to-Applications disk image, for people installing by hand
#   scope-<version>-macos.zip    the same app zipped, which the in-app updater and Homebrew use
#   SHA256SUMS                   checksums of both
#
# Universal (Apple Silicon + Intel). Needs: rustup target add aarch64-apple-darwin x86_64-apple-darwin,
# and cmake (brew install cmake). CI runs it (.github/workflows/release.yml); it runs the same here.
#
# Signing and notarization, so macOS opens the app without warnings (both optional; without them
# the build is ad-hoc signed and only good for testing):
#   SCOPE_SIGN_IDENTITY   "Developer ID Application: Your Name (TEAMID)", in the keychain
#   and one way to reach Apple's notary service:
#     SCOPE_NOTARY_PROFILE                        a profile saved by `xcrun notarytool store-credentials`
#     APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD an app-specific password from appleid.apple.com
#     NOTARY_KEY_PATH, NOTARY_KEY_ID, NOTARY_ISSUER  an App Store Connect API key (.p8)
set -eu

cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
dist=dist
rm -rf "$dist"
mkdir -p "$dist"

SCOPE_UNIVERSAL=1 scripts/bundle.sh -
app=target/release/scope.app

notarize() {
    if [ -n "${SCOPE_NOTARY_PROFILE:-}" ]; then
        set -- "$@" --keychain-profile "$SCOPE_NOTARY_PROFILE"
    elif [ -n "${NOTARY_KEY_PATH:-}" ]; then
        set -- "$@" --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER"
    else
        set -- "$@" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD"
    fi
    xcrun notarytool submit "$@" --wait --timeout 30m
}

can_notarize=no
if [ -n "${SCOPE_SIGN_IDENTITY:-}" ]; then
    if [ -n "${SCOPE_NOTARY_PROFILE:-}${NOTARY_KEY_PATH:-}" ]; then
        can_notarize=yes
    elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
        can_notarize=yes
    fi
fi

zip="$dist/scope-$version-macos.zip"
if [ $can_notarize = yes ]; then
    # Notarize the app, then staple the ticket to it, so it opens offline too.
    ditto -c -k --keepParent "$app" "$dist/notarize.zip"
    notarize "$dist/notarize.zip"
    rm "$dist/notarize.zip"
    xcrun stapler staple "$app"
else
    echo "warning: not notarized (needs SCOPE_SIGN_IDENTITY and notary credentials); macOS will warn when it opens" >&2
fi
ditto -c -k --keepParent "$app" "$zip"

# The disk image: the app and a link to /Applications to drag it onto.
dmg="$dist/scope-$version.dmg"
stage="$dist/dmg"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "scope $version" -srcfolder "$stage" -fs HFS+ -format ULFO -ov "$dmg" >/dev/null
rm -rf "$stage"
if [ -n "${SCOPE_SIGN_IDENTITY:-}" ]; then
    codesign --force --timestamp --sign "$SCOPE_SIGN_IDENTITY" "$dmg"
fi
if [ $can_notarize = yes ]; then
    notarize "$dmg"
    xcrun stapler staple "$dmg"
    spctl --assess --type open --context context:primary-signature -v "$dmg"
fi

(cd "$dist" && shasum -a 256 "scope-$version.dmg" "scope-$version-macos.zip" > SHA256SUMS)
echo "Release files for $version in $dist/:"
ls -lh "$dist"
