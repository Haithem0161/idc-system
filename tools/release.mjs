#!/usr/bin/env node
//
// release.mjs -- cut a new release: bump the three version fields in lockstep,
// commit, tag, and push. The TAG push is what triggers the CI build+sign+deploy
// workflow (.github/workflows/release.yml); this script does versioning + tag +
// push ONLY, nothing heavy.
//
// Usage:
//   pnpm release patch    0.1.0 -> 0.1.1
//   pnpm release minor    0.1.0 -> 0.2.0
//   pnpm release major    0.1.0 -> 1.0.0
//
// Order of operations:
//   1. Refuse unless arg is one of {patch, minor, major}.
//   2. Refuse unless inside a git repo, on `main`, with a clean tree.
//   3. Fetch origin/main and refuse unless local main is up to date (else the
//      push would be rejected AFTER we already committed+tagged -- orphaning
//      the release commit). Refuse if the target tag already exists locally OR
//      on the remote.
//   4. Read the CANONICAL current version from src-tauri/tauri.conf.json and
//      assert package.json and src-tauri/Cargo.toml already agree. Drift aborts
//      with a reconciliation hint.
//   5. Compute next semver; write it into all three files (JSON via parse/
//      stringify; Cargo.toml via a [package]-section-anchored replace that can
//      never touch a dependency's version line).
//   6. Refresh Cargo.lock for the local package (cargo update -p idc-system).
//   7. Commit "chore(release): vX.Y.Z" (no Claude authorship), annotated tag.
//   8. Push branch AND tag ATOMICALLY (git push --atomic) -- both refs land or
//      neither does, so CI never sees a tag whose commit is not yet on origin.
//
// No external dependencies. Node >= 20 (node:util, node:child_process). Reads
// no secrets; the signing key lives only in CI.

import { readFileSync, writeFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"
import { execFileSync } from "node:child_process"

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url))
const ROOT = join(SCRIPT_DIR, "..")

const PKG_JSON = join(ROOT, "package.json")
const TAURI_CONF = join(ROOT, "src-tauri", "tauri.conf.json")
const CARGO_TOML = join(ROOT, "src-tauri", "Cargo.toml")
const CARGO_PKG = "idc-system" // [package] name in src-tauri/Cargo.toml
const DEFAULT_BRANCH = "main"

const BUMPS = new Set(["patch", "minor", "major"])
const SEMVER_RE = /^(\d+)\.(\d+)\.(\d+)$/

function die (msg) {
  process.stderr.write(`release: ${msg}\n`)
  process.exit(1)
}
function info (msg) {
  process.stdout.write(`release: ${msg}\n`)
}

// Run a command inheriting stdio so git/cargo output streams live. No secrets
// are ever passed here.
function run (cmd, args) {
  return execFileSync(cmd, args, { cwd: ROOT, stdio: "inherit" })
}
// Run and capture trimmed stdout (git plumbing reads).
function capture (cmd, args) {
  return execFileSync(cmd, args, { cwd: ROOT, encoding: "utf8" }).trim()
}

function parseSemver (v) {
  const m = SEMVER_RE.exec(v)
  if (!m) die(`'${v}' is not a plain X.Y.Z semver (no pre-release/build metadata supported)`)
  return { major: +m[1], minor: +m[2], patch: +m[3] }
}

function nextVersion (current, kind) {
  const { major, minor, patch } = parseSemver(current)
  switch (kind) {
    case "major": return `${major + 1}.0.0`
    case "minor": return `${major}.${minor + 1}.0`
    case "patch": return `${major}.${minor}.${patch + 1}`
    default: return die(`unknown bump '${kind}'`) // unreachable; guarded earlier
  }
}

function readJsonVersion (path) {
  let parsed
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"))
  } catch (e) {
    return die(`could not parse JSON at ${path}: ${e.message}`)
  }
  if (typeof parsed.version !== "string") die(`no top-level string "version" in ${path}`)
  return parsed.version
}

// Returns { before, body, after } where body is the text between the [package]
// header line and the next top-level "[section]" header (or EOF). Only this
// window is ever edited, so a dependency line like `serde = { version = "1" }`
// further down is physically out of reach.
function extractPackageSection (raw, path) {
  const headerRe = /^[ \t]*\[package\][ \t]*\r?$/m
  const headerMatch = headerRe.exec(raw)
  if (!headerMatch) die(`no [package] section found in ${path}`)
  const bodyStart = headerMatch.index + headerMatch[0].length
  const rest = raw.slice(bodyStart)
  const nextSectionRe = /^[ \t]*\[\[?[^\]]+\]\]?[ \t]*\r?$/m
  const nextMatch = nextSectionRe.exec(rest)
  const bodyEnd = nextMatch ? bodyStart + nextMatch.index : raw.length
  return { before: raw.slice(0, bodyStart), body: raw.slice(bodyStart, bodyEnd), after: raw.slice(bodyEnd) }
}

function readCargoPackageVersion (path) {
  const { body } = extractPackageSection(readFileSync(path, "utf8"), path)
  const m = /^\s*version\s*=\s*"([^"]+)"/m.exec(body)
  if (!m) die(`no version line inside [package] of ${path}`)
  return m[1]
}

// JSON files: parse, set .version, re-stringify (2-space indent, preserve a
// trailing newline). Never regex -- no corruption, single-line diff.
function writeJsonVersion (path, newVersion) {
  const raw = readFileSync(path, "utf8")
  const parsed = JSON.parse(raw)
  parsed.version = newVersion
  const out = JSON.stringify(parsed, null, 2) + (raw.endsWith("\n") ? "\n" : "")
  writeFileSync(path, out)
}

// Cargo.toml: replace ONLY the version line inside the [package] window.
function writeCargoPackageVersion (path, newVersion) {
  const raw = readFileSync(path, "utf8")
  const { before, body, after } = extractPackageSection(raw, path)
  let replaced = false
  const newBody = body.replace(/^(\s*version\s*=\s*")[^"]+(".*)$/m, (_full, pre, post) => {
    replaced = true
    return `${pre}${newVersion}${post}`
  })
  if (!replaced) die(`could not locate the version line inside [package] of ${path}`)
  writeFileSync(path, before + newBody + after)
}

function assertGitRepo () {
  try {
    capture("git", ["rev-parse", "--is-inside-work-tree"])
  } catch {
    die("not inside a git repository")
  }
}
function assertOnDefaultBranch () {
  const branch = capture("git", ["rev-parse", "--abbrev-ref", "HEAD"])
  if (branch !== DEFAULT_BRANCH) die(`must be on '${DEFAULT_BRANCH}' to release (currently on '${branch}')`)
}
function assertCleanTree () {
  if (capture("git", ["status", "--porcelain"]) !== "") {
    die("working tree is not clean -- commit or stash changes before releasing")
  }
}
// Fetch origin/main and refuse if local main is behind (or has diverged). This
// stops the push from being rejected AFTER we have already committed and tagged.
function assertUpToDateWithRemote () {
  info("fetching origin...")
  run("git", ["fetch", "origin", DEFAULT_BRANCH, "--tags", "--prune"])
  const local = capture("git", ["rev-parse", "HEAD"])
  let remote
  try {
    remote = capture("git", ["rev-parse", `origin/${DEFAULT_BRANCH}`])
  } catch {
    return // no remote tracking ref yet (fresh repo) -- nothing to be behind
  }
  if (local !== remote) {
    // Allow the case where remote is an ancestor of local (we are ahead, fine).
    let remoteIsAncestor = false
    try {
      execFileSync("git", ["merge-base", "--is-ancestor", remote, local], { cwd: ROOT })
      remoteIsAncestor = true
    } catch {
      remoteIsAncestor = false
    }
    if (!remoteIsAncestor) {
      die(`local ${DEFAULT_BRANCH} is behind or has diverged from origin/${DEFAULT_BRANCH} -- pull/rebase first`)
    }
  }
}
function assertTagUnused (tag) {
  if (capture("git", ["tag", "--list", tag]) === tag) {
    die(`tag '${tag}' already exists locally -- delete it or pick a different bump`)
  }
  const remote = capture("git", ["ls-remote", "--tags", "origin", `refs/tags/${tag}`])
  if (remote !== "") die(`tag '${tag}' already exists on origin -- pick a different bump`)
}

function main () {
  const kind = process.argv[2]
  if (!kind || !BUMPS.has(kind)) {
    die(`usage: pnpm release <patch|minor|major> (got: ${kind ?? "nothing"})`)
  }

  assertGitRepo()
  assertOnDefaultBranch()
  assertCleanTree()
  assertUpToDateWithRemote()

  // CANONICAL source of truth: tauri.conf.json. The other two must already
  // match it -- catches drift early instead of shipping a half-bumped release.
  const canonical = readJsonVersion(TAURI_CONF)
  parseSemver(canonical)
  const pkgVersion = readJsonVersion(PKG_JSON)
  const cargoVersion = readCargoPackageVersion(CARGO_TOML)

  const mismatches = []
  if (pkgVersion !== canonical) mismatches.push(`  package.json         = ${pkgVersion}`)
  if (cargoVersion !== canonical) mismatches.push(`  src-tauri/Cargo.toml = ${cargoVersion}`)
  if (mismatches.length > 0) {
    die(
      `version drift (canonical src-tauri/tauri.conf.json = ${canonical}):\n` +
      mismatches.join("\n") +
      `\nReconcile all three to the same value in ONE commit, then re-run.`
    )
  }

  const next = nextVersion(canonical, kind)
  const tag = `v${next}`
  assertTagUnused(tag)

  info(`bumping ${kind}: ${canonical} -> ${next}  (tag ${tag})`)

  writeJsonVersion(PKG_JSON, next)
  writeJsonVersion(TAURI_CONF, next)
  writeCargoPackageVersion(CARGO_TOML, next)
  info("wrote package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml")

  // Refresh Cargo.lock for the local package version bump. idc-system is a
  // path package (not on a registry), so a plain `cargo update -p` re-resolves
  // and rewrites only its locked version line; no dependency is touched.
  info("refreshing Cargo.lock...")
  run("cargo", ["update", "-p", CARGO_PKG, "--manifest-path", "src-tauri/Cargo.toml"])

  run("git", ["add", "package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"])
  run("git", ["commit", "-m", `chore(release): ${tag}`])
  run("git", ["tag", "-a", tag, "-m", `Release ${tag}`])

  // Atomic push: branch + tag in one ref transaction. Both land or neither
  // does, so CI never fires on a tag whose commit is not yet on origin, and a
  // rejected push never orphans the release commit locally.
  info("pushing branch and tag (atomic)...")
  run("git", ["push", "--atomic", "origin", DEFAULT_BRANCH, tag])

  info(`released ${tag}. CI will build, sign, and deploy to the VPS.`)
}

main()
