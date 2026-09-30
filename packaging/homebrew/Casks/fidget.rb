# Token is the product name. url, app, quit, zap, and the caveat name the
# bundle in one Release disk image; scripts/bump-homebrew-cask.sh rewrites
# them from that tag. No livecheck: the installable build is a prerelease.
cask "fidget" do
  version "0.0.1-dev"
  sha256 "7d514716900cdf2ccdcedf49982a2d63fb15d16096d8a38f0d51092ea42883c2"

  url "https://github.com/omesser/fidget/releases/download/v#{version}/ai-buddy_#{version}_aarch64.dmg"
  name "Fidget"
  name "ai-buddy"
  desc "Desktop companion that lives on your screen"
  homepage "https://github.com/omesser/fidget"

  depends_on arch: :arm64
  depends_on :macos

  app "ai-buddy.app"

  uninstall quit: "dev.omesser.ai-buddy"

  zap trash: "~/Library/Application Support/ai-buddy"

  caveats <<~EOS
    This build installs ai-buddy. It is not signed. Dismiss the Gatekeeper
    dialog, then System Settings → Privacy & Security → Open Anyway.
  EOS
end
