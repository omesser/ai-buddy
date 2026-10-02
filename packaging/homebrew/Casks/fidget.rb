# Token is the product name. url, app, quit, zap, and the caveat name the
# bundle in one Release disk image; scripts/bump-homebrew-cask.sh rewrites
# them from that tag. livecheck follows GitHub's latest Release.
cask "fidget" do
  version "0.1.0"
  sha256 "9101273fcfc9c8d31b5805e9070d05b73b26f472c3c07cacbf9f6cfac173fd9c"

  url "https://github.com/omesser/fidget/releases/download/v#{version}/Fidget_#{version}_aarch64.dmg"
  name "Fidget"
  desc "Desktop companion that lives on your screen"
  homepage "https://github.com/omesser/fidget"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on arch: :arm64
  depends_on :macos

  app "Fidget.app"

  uninstall quit: "dev.omesser.fidget"

  zap trash: [
    "~/Library/Application Support/fidget",
  ]

  caveats <<~EOS
    This build installs Fidget. It is not signed. Dismiss the Gatekeeper
    dialog, then System Settings → Privacy & Security → Open Anyway.
  EOS
end
