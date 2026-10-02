import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

const script = join(import.meta.dirname, "..", "scripts", "publish-homebrew-cask.sh");

function git(args, cwd) {
  return execFileSync("git", args, { cwd, encoding: "utf8" });
}

function seed(bare, work, body) {
  execFileSync("git", ["init", "--bare", "-b", "main", bare]);
  execFileSync("git", ["clone", "-q", bare, work]);
  mkdirSync(join(work, "packaging", "homebrew", "Casks"), { recursive: true });
  writeFileSync(join(work, "packaging", "homebrew", "Casks", "fidget.rb"), body);
  git(["add", "packaging/homebrew/Casks/fidget.rb"], work);
  git(["config", "user.name", "seed"], work);
  git(["config", "user.email", "seed@example.com"], work);
  git(["config", "commit.gpgsign", "false"], work);
  git(["commit", "-q", "-m", "seed"], work);
  git(["push", "-q", "origin", "HEAD:main"], work);
}

let workSeq = 0;

function fresh(bare) {
  const work = join(bare, "..", `work-${++workSeq}`);
  execFileSync("git", ["clone", "-q", bare, work]);
  return work;
}

function subject(bare, ref) {
  return git(["log", "-1", "--format=%s", ref], bare).trim();
}

function publish(cwd, args) {
  return execFileSync("bash", [script, ...args], { cwd, encoding: "utf8" });
}

test("a matching cask pushes nothing", () => {
  const dir = mkdtempSync(join(tmpdir(), "homebrew-cask-"));
  try {
    const body = "version 0.1.0\n";
    seed(join(dir, "fidget.git"), join(dir, "seed-fidget"), body);
    seed(join(dir, "tap.git"), join(dir, "seed-tap"), "ignored\n");
    const tapBare = join(dir, "tap.git");
    const tapWork = fresh(tapBare);
    mkdirSync(join(tapWork, "Casks"), { recursive: true });
    writeFileSync(join(tapWork, "Casks", "fidget.rb"), body);
    git(["add", "Casks/fidget.rb"], tapWork);
    git(["config", "user.name", "seed"], tapWork);
    git(["config", "user.email", "seed@example.com"], tapWork);
    git(["config", "commit.gpgsign", "false"], tapWork);
    git(["commit", "-q", "-m", "seed tap"], tapWork);
    git(["push", "-q", "origin", "HEAD:main"], tapWork);

    const fidget = fresh(join(dir, "fidget.git"));
    const out = publish(fidget, ["--tag", "v0.1.0", "--tap", fresh(tapBare), "--only", "all"]);
    assert.match(out, /^canonical_status=unchanged$/m);
    assert.match(out, /^tap_status=unchanged$/m);
    assert.equal(subject(join(dir, "fidget.git"), "main"), "seed");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a changed cask is pushed to both remotes and a second run is a no-op", () => {
  const dir = mkdtempSync(join(tmpdir(), "homebrew-cask-"));
  try {
    seed(join(dir, "fidget.git"), join(dir, "seed-fidget"), "version 0.1.0\n");
    seed(join(dir, "tap.git"), join(dir, "seed-tap"), "version 0.1.0\n");
    const fidget = fresh(join(dir, "fidget.git"));
    const tap = fresh(join(dir, "tap.git"));
    mkdirSync(join(tap, "Casks"), { recursive: true });
    writeFileSync(join(tap, "Casks", "fidget.rb"), "version 0.1.0\n");
    git(["add", "Casks/fidget.rb"], tap);
    git(["config", "user.name", "seed"], tap);
    git(["config", "user.email", "seed@example.com"], tap);
    git(["config", "commit.gpgsign", "false"], tap);
    git(["commit", "-q", "-m", "seed tap"], tap);
    git(["push", "-q", "origin", "HEAD:main"], tap);

    writeFileSync(join(fidget, "packaging", "homebrew", "Casks", "fidget.rb"), "version 0.2.0\n");
    const out = publish(fidget, ["--tag", "0.2.0", "--tap", tap, "--only", "all"]);
    assert.match(out, /^canonical_status=pushed$/m);
    assert.match(out, /^tap_status=pushed$/m);
    assert.equal(subject(join(dir, "fidget.git"), "main"), "build: Bump the Homebrew cask to v0.2.0");
    assert.equal(subject(join(dir, "tap.git"), "main"), "fidget: v0.2.0");
    assert.equal(git(["show", "main:Casks/fidget.rb"], join(dir, "tap.git")), "version 0.2.0\n");

    const again = fresh(join(dir, "fidget.git"));
    const tapAgain = fresh(join(dir, "tap.git"));
    const second = publish(again, ["--tag", "v0.2.0", "--tap", tapAgain, "--only", "all"]);
    assert.match(second, /^canonical_status=unchanged$/m);
    assert.match(second, /^tap_status=unchanged$/m);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("an unchanged canonical cask still updates a stale tap", () => {
  const dir = mkdtempSync(join(tmpdir(), "homebrew-cask-"));
  try {
    seed(join(dir, "fidget.git"), join(dir, "seed-fidget"), "version 0.2.0\n");
    seed(join(dir, "tap.git"), join(dir, "seed-tap"), "version 0.1.0\n");
    const tap = fresh(join(dir, "tap.git"));
    mkdirSync(join(tap, "Casks"), { recursive: true });
    writeFileSync(join(tap, "Casks", "fidget.rb"), "version 0.1.0\n");
    git(["add", "Casks/fidget.rb"], tap);
    git(["config", "user.name", "seed"], tap);
    git(["config", "user.email", "seed@example.com"], tap);
    git(["config", "commit.gpgsign", "false"], tap);
    git(["commit", "-q", "-m", "seed tap"], tap);
    git(["push", "-q", "origin", "HEAD:main"], tap);

    const out = publish(fresh(join(dir, "fidget.git")), [
      "--tag",
      "v0.2.0",
      "--tap",
      tap,
      "--only",
      "all",
    ]);
    assert.match(out, /^canonical_status=unchanged$/m);
    assert.match(out, /^tap_status=pushed$/m);
    assert.equal(git(["show", "main:Casks/fidget.rb"], join(dir, "tap.git")), "version 0.2.0\n");
    assert.equal(subject(join(dir, "fidget.git"), "main"), "seed");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a refused default branch updates the bump branch and still pushes the tap", () => {
  const dir = mkdtempSync(join(tmpdir(), "homebrew-cask-"));
  try {
    const fidgetBare = join(dir, "fidget.git");
    seed(fidgetBare, join(dir, "seed-fidget"), "version 0.1.0\n");
    const tapBare = join(dir, "tap.git");
    seed(tapBare, join(dir, "seed-tap"), "version 0.1.0\n");
    writeFileSync(
      join(fidgetBare, "hooks", "update"),
      "#!/bin/sh\nif [ \"$1\" = refs/heads/main ]; then echo 'Changes must be made through a pull request.' >&2; exit 1; fi\n",
    );
    execFileSync("chmod", ["+x", join(fidgetBare, "hooks", "update")]);

    const fidget = fresh(fidgetBare);
    const tap = fresh(tapBare);
    mkdirSync(join(tap, "Casks"), { recursive: true });
    writeFileSync(join(tap, "Casks", "fidget.rb"), "old\n");
    git(["add", "Casks/fidget.rb"], tap);
    git(["config", "user.name", "seed"], tap);
    git(["config", "user.email", "seed@example.com"], tap);
    git(["config", "commit.gpgsign", "false"], tap);
    git(["commit", "-q", "-m", "seed tap"], tap);
    git(["push", "-q", "origin", "HEAD:main"], tap);

    writeFileSync(join(fidget, "packaging", "homebrew", "Casks", "fidget.rb"), "version 0.2.0\n");
    const out = publish(fidget, ["--tag", "v0.2.0", "--tap", tap, "--only", "all"]);
    assert.match(out, /^canonical_status=pull-request$/m);
    assert.match(out, /^canonical_branch=homebrew-cask-bump$/m);
    assert.match(out, /^tap_status=pushed$/m);
    assert.equal(subject(fidgetBare, "main"), "seed");
    assert.equal(git(["show", "homebrew-cask-bump:packaging/homebrew/Casks/fidget.rb"], fidgetBare), "version 0.2.0\n");
    assert.equal(git(["show", "main:Casks/fidget.rb"], tapBare), "version 0.2.0\n");

    const fidget2 = fresh(fidgetBare);
    const tap2 = fresh(tapBare);
    writeFileSync(join(fidget2, "packaging", "homebrew", "Casks", "fidget.rb"), "version 0.2.0\n");
    const second = publish(fidget2, ["--tag", "v0.2.0", "--tap", tap2, "--only", "all"]);
    assert.match(second, /^canonical_status=pull-request$/m);
    assert.match(second, /^tap_status=unchanged$/m);
    assert.equal(subject(fidgetBare, "main"), "seed");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a tag that is not a release is refused before any push", () => {
  const dir = mkdtempSync(join(tmpdir(), "homebrew-cask-"));
  try {
    seed(join(dir, "fidget.git"), join(dir, "seed-fidget"), "version 0.1.0\n");
    const fidget = fresh(join(dir, "fidget.git"));
    assert.throws(
      () => publish(fidget, ["--only", "canonical", "--tag", "not a tag", "--branch", "main"]),
      (err) => err.status === 2 && /not a release tag/.test(err.stderr),
    );
    assert.equal(subject(join(dir, "fidget.git"), "main"), "seed");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
