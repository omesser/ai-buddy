# Homebrew release shape for the macOS app

A third-party tap that installs `Fidget.app` from the GitHub Release disk
image is the Homebrew package for this build. The build is ad-hoc signed, so
`homebrew/cask` does not accept it. The install line is:

```sh
brew install --cask omesser/fidget/fidget
```

Shipped in #1204, merged as `207fe9f0` on `main` (2026-10-02). Every Fidget
citation below is read against that commit. Homebrew sources are pinned
beside the claim.

---

## The package

`packaging/homebrew/Casks/fidget.rb` names cask `fidget`. It downloads
`Fidget_<version>_aarch64.dmg` from the GitHub Release, checks sha256,
installs `Fidget.app`, quits `dev.omesser.fidget`, and zaps
`~/Library/Application Support/fidget`. It depends on Apple Silicon and
macOS. `livecheck` uses `:github_latest`, which follows the marked Latest
release and skips drafts and prereleases.

Homebrew sends a native macOS application to a cask, and open-source
command-line software to a formula
([Adding Software to Homebrew](https://docs.brew.sh/Adding-Software-to-Homebrew)).
Acceptable Casks puts command-line-only open source in `homebrew/core`, and
puts graphical open source there too when it has no current compiled
distribution
([Acceptable Casks](https://docs.brew.sh/Acceptable-Casks), "Appropriate
package type"). Fidget publishes a disk image. The `app` stanza is what
moves that bundle into `/Applications`
([Cask Cookbook](https://docs.brew.sh/Cask-Cookbook), `app`).

DVC is the formula case. Its README says `brew install dvc` and badges
`https://formulae.brew.sh/formula/dvc`
([`README.rst`](https://github.com/treeverse/dvc/blob/56e59829512ff134aa269099a2099587b810b4dd/README.rst)
at `56e59829`). The package in `homebrew/core` is `class Dvc < Formula`,
built from a PyPI sdist, with bottles for Apple Silicon and for Linux
([`Formula/d/dvc.rb`](https://github.com/Homebrew/homebrew-core/blob/a6fad76ef3bdf165b5de8b0941783f58749caf5d/Formula/d/dvc.rb)
at core `a6fad76e`). `Casks/d/dvc.rb` on homebrew-cask returns 404, and
`Formula/dvc.rb` in the DVC repository at `56e59829` returns 404. A formula
installs into the Cellar and links executables. That is the package for a
command, and the wrong package for `Fidget.app`.

## Why the tap is `omesser/homebrew-fidget`

`homebrew/cask` requires a macOS app to pass Gatekeeper, and rejects a cask
that needs Gatekeeper disabled or bypassed
([Acceptable Casks](https://docs.brew.sh/Acceptable-Casks), "Platform
compatibility and macOS security protections"). The signing audit on an
official tap reports: "The homebrew/cask tap requires all casks to be signed
and notarized by Apple."
([`cask/audit.rb`](https://github.com/Homebrew/brew/blob/d6ca35509427ed7b4e37445054fa343ab90755ec/Library/Homebrew/cask/audit.rb#L600-L688)
at brew `d6ca3550`, 2026-10-01). `src-tauri/tauri.conf.json` sets
`bundle.macOS.signingIdentity` to `"-"`. Tauri documents that value as an
ad-hoc signature, and says ad-hoc signing still asks the user to allow the
app under Privacy & Security
([macOS Code Signing](https://v2.tauri.app/distribute/sign/macos/), "Ad-Hoc
Signing").

A self-submission also needs at least 90 forks, 90 watchers, or 225 stars on
the canonical repository
([Package Acceptance Policy](https://docs.brew.sh/Package-Acceptance-Policy),
"Notability"). On 2026-10-02 `omesser/fidget` had 2 stars, 0 forks, and 0
watchers. Software outside the official rules is maintained in a third-party
tap, and that tap is not a Homebrew endorsement
([Package Acceptance Policy](https://docs.brew.sh/Package-Acceptance-Policy),
"Scope and third-party taps";
[How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
"Upstream taps").

`brew tap user/repo` clones `https://github.com/user/homebrew-repo`
([`brew` manual](https://docs.brew.sh/Manpage), `tap`). `brew tap
omesser/fidget` clones `omesser/homebrew-fidget`. Cask files live in a
top-level `Casks/` directory
([How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
"Casks"). The tap holds `Casks/fidget.rb`. This repository keeps the verified
copy at `packaging/homebrew/Casks/fidget.rb`. Tapping the application
repository finds no cask, because that path is not the top-level `Casks/`
directory Homebrew reads.

`brew install --cask omesser/fidget/fidget` is the direct install. Since
Homebrew 6.0.0 that fully qualified name trusts only the cask
([Tap Trust](https://docs.brew.sh/Tap-Trust)). `brew update` fetches the tap,
and `brew upgrade --cask fidget` installs the version the tap names
([How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap),
"Updating"; [`brew` manual](https://docs.brew.sh/Manpage), `upgrade`).

## Release to tap

A published full Release runs the `homebrew` job in
`.github/workflows/release.yml`. The job requires
`HOMEBREW_TAP_DEPLOY_KEY`, checks out the default branch, runs
`scripts/bump-homebrew-cask.sh` and `scripts/verify-homebrew-cask.sh` for the
Release tag, checks out `omesser/homebrew-fidget` with that key, and
`scripts/publish-homebrew-cask.sh` copies `Casks/fidget.rb` onto the tap. A
prerelease does not run the job. The tap push does not wait on the cask pull
request this repository opens when the default branch requires one.
Dispatching the Release workflow with `sync-tap-tag` set reconciles a tag
that is already published and skips the package build. `docs/DEVELOPMENT.md`
("Homebrew") records the same path. `:github_latest` reports the marked
Latest release. The bump is what publishes the new `version` and `sha256` to
the tap.
