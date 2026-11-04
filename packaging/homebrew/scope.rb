# Template for the Homebrew cask. The release workflow fills in REPO, VERSION and SHA256 and
# pushes the result to the tap as Casks/scope.rb, so `brew install --cask` gets each release.
cask "scope" do
  version "VERSION"
  sha256 "SHA256"

  url "https://github.com/REPO/releases/download/v#{version}/scope-#{version}.dmg"
  name "scope"
  desc "Desktop IDE for running, tracking and steering Claude Code sessions"
  homepage "https://github.com/REPO"

  livecheck do
    url :url
    strategy :github_latest
  end

  # scope updates itself (Settings > About); brew upgrade works too.
  auto_updates true
  depends_on macos: ">= :big_sur"

  app "scope.app"
  # The `scope` command (list, rename, move, new, ask) in the terminal.
  binary "#{appdir}/scope.app/Contents/MacOS/scope"

  caveats <<~EOS
    scope runs your Claude Code sessions, so it needs Claude Code itself:
      https://docs.claude.com/en/docs/claude-code
  EOS

  zap trash: "~/.config/scope"
end
