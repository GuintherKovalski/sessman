# Releasing scope

A release is a universal (Apple Silicon + Intel) `scope.app`, signed with a Developer ID and
notarized by Apple, published on GitHub as:

| File | For |
|---|---|
| `scope-<version>.dmg` | people installing by hand (drag to Applications) |
| `scope-<version>-macos.zip` | the in-app updater and `install.sh` |
| `SHA256SUMS` | checksums of both |
| `install.sh` | `curl -fsSL …/releases/latest/download/install.sh \| sh` |

…plus the Homebrew cask in the tap, pointing at the new `.dmg`.

## Making one

```sh
scripts/flow.sh release 0.2.0
```

It bumps the version in `Cargo.toml` on `feature/release-0.2.0` and ships it to stage, like any
change. When CI has tested stage and promoted it to main, it sees the new version and runs
`.github/workflows/release.yml`, which tags `v0.2.0`, builds, signs, notarizes, publishes the
release and updates the cask. Watch it with `gh run watch`. Nobody tags by hand (the pre-push
hook refuses `v*` tags). To redo a release that failed, run the **release** workflow from the
Actions tab with the version.

Versions are `MAJOR.MINOR.PATCH`. Installed copies of scope offer any release with a higher
version than theirs; drafts and pre-releases are never offered.

## One-time setup

Until this is done, releases still build, but they are only ad-hoc signed: macOS refuses to
open them without a trip to System Settings → Privacy & Security → Open Anyway, and Homebrew
installs hit the same wall. That's not something to hand to other people.

### 1. Make the downloads public

Release files of a private repository can't be downloaded without a GitHub login, so the
updater, `install.sh` and Homebrew all fail for everyone else. Either:

- make this repository public, or
- keep it private and publish releases in a public one (e.g. `GuintherKovalski/scope`): create
  it (empty is fine), set the variable `RELEASE_REPO` to it, and add a `RELEASE_TOKEN` secret:
  a fine-grained token with **Contents: read and write** on that repository.

The app looks for updates in `GuintherKovalski/sessman` unless it was built with
`SCOPE_RELEASE_REPO`; the workflow passes `RELEASE_REPO`, so released builds look in the right
place. If you change it, also update the links in the README's Install section.

### 2. Developer ID certificate (Apple Developer Program, $99/year)

1. Join at [developer.apple.com/programs](https://developer.apple.com/programs/).
2. Xcode → Settings → Accounts → Manage Certificates → **+** → *Developer ID Application*
   (or create it at developer.apple.com → Certificates).
3. Keychain Access → My Certificates → right-click it → Export → `.p12` with a password.
4. Add the secrets:

   ```sh
   base64 -i DeveloperID.p12 | gh secret set MACOS_CERTIFICATE
   gh secret set MACOS_CERTIFICATE_PASSWORD
   ```

### 3. Notarization

Pick one:

- **App-specific password**: create one at [account.apple.com](https://account.apple.com) →
  Sign-In and Security → App-Specific Passwords, then
  `gh secret set APPLE_ID`, `gh secret set APPLE_TEAM_ID` (10 characters, shown at
  developer.apple.com → Membership), `gh secret set APPLE_APP_PASSWORD`.
- **App Store Connect API key** (doesn't depend on a person's account): App Store Connect →
  Users and Access → Integrations → Keys, role *Developer*; then
  `base64 -i AuthKey_XXXX.p8 | gh secret set NOTARY_KEY`, `gh secret set NOTARY_KEY_ID`,
  `gh secret set NOTARY_ISSUER`.

### 4. Homebrew tap

1. Create the public repository `GuintherKovalski/homebrew-scope` (the `homebrew-` prefix is
   what makes `brew install --cask guintherkovalski/scope/scope` find it). Another name: set
   the variable `HOMEBREW_TAP`.
2. Add a `TAP_TOKEN` secret: a fine-grained token with **Contents: read and write** on the tap.

The workflow writes `Casks/scope.rb` from `packaging/homebrew/scope.rb` on every release.
The cask also links the `scope` command into the PATH.

## Building a release locally

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
brew install cmake
scripts/release.sh                       # unsigned, into dist/
SCOPE_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)" \
SCOPE_NOTARY_PROFILE=scope scripts/release.sh   # after: xcrun notarytool store-credentials scope
```

Check a signed build the way Gatekeeper will:

```sh
spctl --assess --type execute -v target/release/scope.app   # "accepted, source=Notarized Developer ID"
xcrun stapler validate dist/scope-*.dmg
```

## What gets installed where

- The app: `/Applications/scope.app` (or `~/Applications` with `install.sh` when `/Applications`
  isn't writable). The updater replaces it in place, so it must be writable by the user.
- Settings, workspaces, state and the Whisper model: `~/.config/scope` (`brew uninstall --zap`
  removes it). The sessions themselves stay in Claude Code's `~/.claude`.
