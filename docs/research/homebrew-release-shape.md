# Homebrew release shape for the macOS app

The cask on PR #1204 is the package Homebrew documents for a native macOS
application shipped as the developer's own disk image, and a third-party tap
is the channel that can carry this build. `homebrew/cask` requires a
Gatekeeper pass and a notability bar this repository does not meet. Two parts
of that pull request sit outside the shape. Current Homebrew refuses
`brew install` of a cask downloaded from an `https` URL, which is the one-shot
line in the README and in `docs/DEVELOPMENT.md`. And `brew update` reads
`omesser/homebrew-fidget`, which this repository's workflow cannot push. That
tap is already one commit behind the pull request tip.

Anchor: `a6ba6489` on `cursor/homebrew-cask-3a14`, the tip of
[PR #1204](https://github.com/omesser/fidget/pull/1204), 2026-10-02. `main` at
`338d135a` is an ancestor of that tip and has no cask. Every Fidget citation
below is read against `a6ba6489`. Homebrew sources are pinned beside the claim.
This note does not change the cask.

---

## What the tip actually contains

`packaging/homebrew/Casks/fidget.rb` at `a6ba6489`:

```4:33:packaging/homebrew/Casks/fidget.rb
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
```

The same commit's README Get It and `docs/DEVELOPMENT.md` lead with
`brew install --cask omesser/fidget/fidget`, name the tap
[omesser/homebrew-fidget](https://github.com/omesser/homebrew-fidget), and
keep a raw-URL install beside it. `.github/workflows/homebrew-cask.yml`
checks the in-repo file on a Linux runner (`contents: read` only).
`scripts/bump-homebrew-cask.sh` rewrites that file from a tag. The workflow
comment and the development doc both say a person copies the file into the
tap afterwards.

The installable tap commit is `981b0db1` (2026-10-02, "build: Add the Fidget
0.1.0 cask"). Its `Casks/fidget.rb` matches the block above except `zap`,
which still lists `~/Library/Application Support/ai-buddy` as well as
`fidget`. `a6ba6489` dropped the pre-rename leaf.

Release `v0.1.0` is the GitHub latest release, not a draft and not a
prerelease. Its assets are `Fidget_0.1.0_aarch64.dmg`,
`Fidget_0.1.0_amd64.AppImage`, `Fidget_0.1.0_amd64.deb`, and
`Fidget_0.1.0_x64-setup.exe`. The two older releases, `v0.0.1-dev` and
`v0.0.0-test`, are prereleases. Measured from
`GET /repos/omesser/fidget/releases` and `/releases/latest` on 2026-10-02.
The disk image's sha256 is
`9101273fcfc9c8d31b5805e9070d05b73b26f472c3c07cacbf9f6cfac173fd9c`, matching
the cask. Inside it, `Fidget.app` is a Mach-O arm64 executable,
`CFBundleIdentifier` is `dev.omesser.fidget`,
`CFBundleShortVersionString` is `0.1.0`, and `LSMinimumSystemVersion` is
`10.13`.

`src-tauri/tauri.conf.json` sets `bundle.macOS.signingIdentity` to `"-"`,
`createUpdaterArtifacts` to false, and an updater endpoint at
`releases/latest/download/latest.json`. That `latest.json` is not a `v0.1.0`
asset. `check_for_update` still runs at startup and will download an update
when the plugin reports one (`src-tauri/src/main.rs`).

## Tap, for this build

Homebrew's official repositories are `homebrew/core` and `homebrew/cask`.
Software that fails their rules belongs in a third-party tap, and a
third-party tap is not a Homebrew endorsement
([Package Acceptance Policy](https://docs.brew.sh/Package-Acceptance-Policy),
[How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
"Upstream taps").

Two official rules close `homebrew/cask` to the `v0.1.0` disk image.

Gatekeeper. A macOS cask in `homebrew/cask` must pass Homebrew's Gatekeeper
checks, and the app must not require Gatekeeper to be disabled or bypassed
([Acceptable Casks](https://docs.brew.sh/Acceptable-Casks), "Platform
compatibility and macOS security protections"). The audit that enforces it
runs for an official tap and skips a third-party tap unless signing audit is
asked for. On failure in an official tap the message is: "The homebrew/cask
tap requires all casks to be signed and notarized by Apple."
([`cask/audit.rb`](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/Library/Homebrew/cask/audit.rb#L600-L688)
at brew `d6ca3550`, 2026-10-01). Tauri documents `signingIdentity: "-"` as an
ad-hoc signature, and says ad-hoc signing still makes macOS ask the user to
allow the app under Privacy & Security
([macOS Code Signing](https://v2.tauri.app/distribute/sign/macos/), "Ad-Hoc
Signing"). The extracted app has `Contents/_CodeSignature`. This note did not
run `gktool` or `codesign`; the machine is Linux. The caveat's "Open Anyway"
instruction is the whitelist Tauri describes. The phrase "It is not signed"
is looser than the config: the build is ad-hoc signed, and it is not
notarized. Tightening that sentence would not change the install shape.
Stripping the quarantine bit from the installed app, which AeroSpace's tap
cask does in `postflight_steps`, is the bypass Acceptable Casks rejects for
`homebrew/cask`. #1204 does not do that.

Notability. A self-submission needs at least 90 forks, 90 watchers, or 225
stars on the canonical repository, and a repository younger than 30 days is
normally ineligible
([Package Acceptance Policy](https://docs.brew.sh/Package-Acceptance-Policy),
"Notability"). `omesser/fidget` was created 2026-08-25 and on 2026-10-02 had
2 stars, 0 forks, and 0 watchers (`gh api repos/omesser/fidget`). Age clears
the 30-day line. The counts do not clear a self-submission. A signed build
would still be short of `homebrew/cask` on this rule. Maintainer discretion
can waive a criterion; the policy says that is an exception, not the default.

No cask file named `fidget.rb` turned up in `Homebrew/homebrew-cask` (code
search, 2026-10-02, `total_count` 0). The token `fidget` follows the cookbook:
lowercase, the `.app` name, no version suffix
([Cask Cookbook](https://docs.brew.sh/Cask-Cookbook), "Token reference").
Casks in a tap need globally unique names so two taps do not clash; prefixing
with the GitHub user is one way to get that
([How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
"Casks"). Uniqueness holds today. A prefix becomes the cookbook's rule only
once another cask already holds the token.

## What `brew` installs

A tap hosted on GitHub is conventionally named `homebrew-<repo>`. `brew tap
user/repo` clones `https://github.com/user/homebrew-repo`
([`brew` manual](https://docs.brew.sh/Manpage), `tap`). `omesser/fidget`
therefore names `omesser/homebrew-fidget`. Cask files live in a top-level
`Casks/` directory
([How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
"Casks"). The tap has `Casks/fidget.rb`. `packaging/homebrew/Casks/` inside
this repository is not that layout, so tapping `omesser/fidget` the
application repo would not find a cask.

`brew install --cask omesser/fidget/fidget` is the direct install the tap
guide recommends. Since Homebrew 6.0.0 a non-official tap is untrusted until
granted, and that fully qualified install trusts only the cask
([Tap Trust](https://docs.brew.sh/Tap-Trust)). `brew update` fetches tapped
repositories; `brew upgrade` upgrades the installed cask from the version in
the updated tap
([How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
"Updating"; [`brew` manual](https://docs.brew.sh/Manpage), `upgrade`).
`livecheck` is a separate report of the upstream version. It does not edit
the tap, so a new GitHub Release reaches `brew upgrade` when the tap's
`version` and `sha256` change.

`depends_on arch: :arm64` is the cookbook's single-architecture form, and
`depends_on :macos` marks the cask macOS-only
([Cask Cookbook](https://docs.brew.sh/Cask-Cookbook), `depends_on`). The
Release has one macOS disk image, and it is arm64. Homebrew's own install
requirements, read from brew `d6ca3550`
[`docs/Installation.md`](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/docs/Installation.md),
support macOS Sequoia (15) and newer; Catalina and older do not run Homebrew.
The app's `LSMinimumSystemVersion` of 10.13 sits under that floor, so a
tighter `depends_on macos:` would not change who can install it.

`app "Fidget.app"` is the artifact stanza for a bundle that should land in
`/Applications`. `uninstall quit:` takes the bundle id and sends the quit
Apple Event on uninstall and on upgrade
([Cask Cookbook](https://docs.brew.sh/Cask-Cookbook), `app`, `uninstall`).
The id matches `CFBundleIdentifier` in the disk image and `identifier` in
`tauri.conf.json`.

`zap` runs only for `brew uninstall --zap`. The trash path matches
`data_dir()`, which joins the platform data directory with `"fidget"`:

```35:39:crates/core/src/memory.rs
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("fidget")
}
```

On macOS, `dirs::data_dir` is `$HOME/Library/Application Support`
([`dirs` `data_dir`](https://docs.rs/dirs/latest/dirs/fn.data_dir.html)).
The cookbook's usual extra locations (caches, logs, preferences, saved state)
are a `brew generate-zap` pass after a launch. This note did not launch the
app, so it does not claim those paths are empty or full. That pass would add
trash entries. It would not change the cask's role.

`auto_updates true` tells `brew upgrade` to skip the cask unless `--greedy`
or `--greedy-auto-updates` is set
([`brew` manual](https://docs.brew.sh/Manpage), `upgrade`;
[Cask Cookbook](https://docs.brew.sh/Cask-Cookbook), `auto_updates`: set it
when the app itself downloads and installs the update). `check_for_update`
does call `download_and_install`, and `v0.1.0` publishes no updater artifact
for that call to find. Setting `auto_updates` now would make `brew upgrade`
skip the cask while the in-app updater also has nothing to install. The
stanza belongs on the cask once a Release actually ships `latest.json` and
the updater payload.

## `livecheck` with `:github_latest`

The strategy is opt-in. Its priority is 0, so livecheck skips it unless the
block says `strategy :github_latest`. It applies to a GitHub URL, including a
release-asset URL of the form
`https://github.com/<user>/<repo>/releases/download/...`, and it requests
`GET /repos/<user>/<repo>/releases/latest`. The default regex is
`/v?(\d+(?:\.\d+)+)/i`, and the version it keeps is the first capture group
of `tag_name`
([`github_latest.rb`](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/Library/Homebrew/livecheck/strategy/github_latest.rb)
and
[`github_releases.rb`](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/Library/Homebrew/livecheck/strategy/github_releases.rb)
at brew `d6ca3550`). `v0.1.0` yields `0.1.0`, which is the cask's `version`.
The cask URL is interpolated as `v#{version}`, so the tag prefix and the
version stanza stay aligned.

Homebrew's livecheck guide allows `GithubLatest` when the repository has a
latest release for a suitable version and the formula or cask uses a release
asset, or when the Git strategy would identify an unreleased version
([`brew livecheck`](https://docs.brew.sh/Brew-Livecheck)). Both reasons apply.
The disk image is a release asset. The Git strategy has priority 8 and
rewrites a `github.com/<user>/<repo>/releases/download/...` URL to
`https://github.com/<user>/<repo>.git`, then lists tags
([`git.rb`](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/Library/Homebrew/livecheck/strategy/git.rb)
at the same brew commit). Fidget already publishes prerelease tags.

GitHub's `GET /repos/{owner}/{repo}/releases/latest` returns the latest
published full release: the most recent non-prerelease, non-draft, and drafts
and prereleases cannot be marked latest
([REST API, "Get the latest release"](https://docs.github.com/en/rest/releases/releases#get-the-latest-release)
and the `make_latest` body parameter on "Create a release"). A publisher can
point "latest" at an older full release with `make_latest`. The strategy
follows that mark. It does not scan every tag.

The same block is what official casks use for a GitHub Release disk image.
Pot, a Tauri app (`tauri` 1.8 in
[`pot-desktop` `src-tauri/Cargo.toml`](https://github.com/pot-app/pot-desktop/blob/594d32ede96acd106b0256deaa8bb440ffcdff40/src-tauri/Cargo.toml)),
is cask `pot` in `homebrew/cask` at `b2744ae2` (2026-10-02): `url :url`,
`strategy :github_latest`, a `releases/download` disk image, `app`, and
`depends_on :macos`
([`Casks/p/pot.rb`](https://github.com/Homebrew/homebrew-cask/blob/b2744ae2f7ee07250376c3f12a2f0f4110354c11/Casks/p/pot.rb)).
Pot publishes arm and Intel disk images, so the cask uses `arch` and two
checksums. Fidget publishes one macOS disk image, so a single `sha256` plus
`depends_on arch: :arm64` is that pattern with the missing architecture left
out. Adding an `arch` hash before an Intel image exists would point `url` at
a file the Release does not contain.

Zed's cask at the same homebrew-cask commit downloads a GitHub Release disk
image and does not use `:github_latest`. Its livecheck reads Zed's stable
channel JSON, and the macOS half sets `auto_updates true` and
`uninstall quit: "dev.zed.Zed"`
([`Casks/z/zed.rb`](https://github.com/Homebrew/homebrew-cask/blob/b2744ae2f7ee07250376c3f12a2f0f4110354c11/Casks/z/zed.rb)).
Acceptable Casks says the unversioned cask tracks the channel recommended for
most users, which need not be the newest release. Fidget's channel for most
users is the marked GitHub latest release, which is the case `:github_latest`
is for. A preview channel later would be a second token with an `@preview`
suffix ([Cask Cookbook](https://docs.brew.sh/Cask-Cookbook), "Casks for
alternative release channels"), not a change to this block.

## DVC is a formula because DVC is a command

[Adding Software to Homebrew](https://docs.brew.sh/Adding-Software-to-Homebrew)
sends open-source command-line software to a formula and a native macOS
application to a cask. Acceptable Casks repeats it: command-line-only
open source normally belongs in `homebrew/core`, and so does open-source
graphical software that has no current compiled distribution. A compiled
distribution the developer publishes is what a cask downloads.

DVC's README tells macOS users to run `brew install dvc` and badges
`https://formulae.brew.sh/formula/dvc`
([`README.rst`](https://github.com/treeverse/dvc/blob/56e59829512ff134aa269099a2099587b810b4dd/README.rst)
at `56e59829`). The formula is `class Dvc < Formula` in `homebrew/core`,
built from a PyPI sdist (`dvc-3.67.1.tar.gz` at core `a6fad76e`, 2026-10-02),
with bottles for Apple Silicon and for Linux
([`Formula/d/dvc.rb`](https://github.com/Homebrew/homebrew-core/blob/a6fad76ef3bdf165b5de8b0941783f58749caf5d/Formula/d/dvc.rb)).
`Casks/d/dvc.rb` on homebrew-cask `main` returns 404, and
`Formula/dvc.rb` in the DVC repository at `56e59829` returns 404. DVC also
ships its own platform packages from the GitHub Release page; the Homebrew
path in the README is the core formula, maintained in `homebrew/core`, not a
file in `treeverse/dvc`.

That is the wrong template for `Fidget.app`. A formula installs into the
Cellar and links executables. The `app` stanza is what moves a bundle into
`/Applications`. #260's Linuxbrew line stays open: this cask declares
`depends_on :macos`, and the Linux artifacts on `v0.1.0` are an AppImage and
a `.deb`. An `app_image` stanza is Linux-only
([Cask Cookbook](https://docs.brew.sh/Cask-Cookbook), artifact table). It
would be a second package. It is not a reason to turn the macOS app into a
formula. Pacman and Chocolatey stay out of this note, as they are out of
#1204.

## Where other macOS apps put the file

Homebrew's instruction to an upstream is to commit the cask in the tap and
push that repository
([How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
"Creating your formula or cask"). `brew update` then sees it. Official
upstreams that `homebrew/cask` has accepted do not keep a second copy as the
install source: Pot's cask is `Casks/p/pot.rb` in homebrew-cask, Zed's is
`Casks/z/zed.rb` there, and the Zed repository root at `2a97fbf2` has no
`Casks` directory.

Apps that stay on a private tap still publish the tap repository, not an
`https` URL of a Ruby file.

AeroSpace's README prefers `brew install --cask nikitabobko/tap/aerospace`
([`README.md`](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/README.md)
at `74a1bf17`). The cask lives in
[`nikitabobko/homebrew-tap` `Casks/aerospace.rb`](https://github.com/nikitabobko/homebrew-tap/blob/9ac0bfc08904719c52a63101fa7e1e133a23bda2/Casks/aerospace.rb)
(`9ac0bfc0`). `script/build-brew-cask.sh` in the app repo writes a generated
Ruby file under `.release/`, and the generated file is marked as generated
([script](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/script/build-brew-cask.sh)).
`Casks/aerospace.rb` inside the AeroSpace repo returns 404. The generator
does not push the tap. The README does not offer a raw cask URL.

Pot's README still tells macOS users to `brew tap pot-app/homebrew-tap` and
then `brew install --cask pot`
([`README.md`](https://github.com/pot-app/pot-desktop/blob/594d32ede96acd106b0256deaa8bb440ffcdff40/README.md)
at `594d32ed`). That tap's cask, at `c2156579`, has the same version and
checksums as the homebrew-cask copy and has no `livecheck` block
([`Casks/pot.rb`](https://github.com/pot-app/homebrew-tap/blob/c21565797818b0f0ff9784f8ca4502a4bfa7b3f4/Casks/pot.rb)).
`Casks/pot.rb` inside pot-desktop returns 404. The same token in
`homebrew/cask` and in a private tap is the clash the tap guide says to
avoid. Fidget does not have that second copy in `homebrew/cask`.

DVC, Zed, Pot, and AeroSpace agree on one point: the file `brew update` reads
is the file in the tap (`homebrew/core`, `homebrew/cask`, or
`user/homebrew-*`). A generator or a checked-in Ruby file in the application
repo is a build input. #1204 already describes the in-repo file that way in
`docs/DEVELOPMENT.md`, and then also offers the raw URL as an install.

## Gaps that would change #1204

**The raw URL is not an install.** `Cask::CaskLoader::FromURILoader` allows
only the `file` scheme. Any other scheme, including `https`, raises
`UnsupportedInstallationMethod` before the download: "Non-checksummed
download of fidget.rb formula file from an arbitrary URL is unsupported!"
([`cask_loader.rb`](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/Library/Homebrew/cask/cask_loader.rb#L21)
and
[#L285-L289](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/Library/Homebrew/cask/cask_loader.rb#L285-L289)
at brew `d6ca3550`). The formula loader raises the same error
([`formulary.rb`](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/Library/Homebrew/formulary.rb#L696-L703)).
The README and `docs/DEVELOPMENT.md` on `a6ba6489` describe that URL as a
one-shot that does not upgrade. On this Homebrew it does not install. This
note did not execute `brew`; the loader is the source. Dropping that line
from the documented install path is the change. The tap command is already
the one the tap guide recommends.

**The tap commit is the release.** `brew update` / `brew upgrade --cask
fidget` follow `omesser/homebrew-fidget` `Casks/fidget.rb`. `livecheck` will
report a new tag and will not publish it. #260 accepts a manual publish path
until CI does it, and the development doc states that path. The path has
already missed a commit: tap `981b0db1` still zaps `ai-buddy`; pull request
tip `a6ba6489` does not. A bump is installed only when that tap file matches
the file CI verified. Keeping `packaging/homebrew/Casks/fidget.rb` as the
verified input matches how AeroSpace generates a cask in the app repo. The
design change is on the publish side: the tap push (by hand, or by a
credential this workflow does not have) is part of the bump, and the raw URL
is not a fallback for a tap that was not pushed. `brew bump-cask-pr` opens a
pull request against the cask's tap
([`brew` manual](https://docs.brew.sh/Manpage), `bump-cask-pr`); it is the
official-tap equivalent of that push, and it targets a repository the
workflow can write.

Nothing in the stanza set above needs to change for the macOS GUI release:
token, versioned GitHub Release URL, sha256, `:github_latest`, arm64-only,
`app`, quit by bundle id, zap of `data_dir()`, and a third-party tap. Intel
waits on an Intel disk image. `homebrew/cask` waits on notarization and on
notability. `auto_updates` waits on an updater artifact the app can install.
