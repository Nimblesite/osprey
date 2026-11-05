---
name: upgrade-packages
description: Upgrade all dependencies/packages to their latest versions for the detected language(s). Use when the user says "upgrade packages", "update dependencies", "bump versions", "update packages", or "upgrade deps".
argument-hint: "[--check-only] [--major] [package-name]"
---
<!-- agent-pmo:74cf183 -->

# Upgrade Packages

Upgrade all project dependencies to their latest compatible (or latest major, if `--major`) versions.

## Arguments

- `--check-only` — List outdated packages without upgrading. Stop after Step 2.
- `--major` — Include major version bumps (breaking changes). Without this flag, stay within semver-compatible ranges.
- Any other argument is treated as a specific package name to upgrade (instead of all packages).

## Step 1 — Detect language and package manager

This repo has multiple manifests:

| Location | Manifest file | Language | Package manager |
|---|---|---|---|
| `src/toolchain/` (workspace: `crates/*`, `tree-sitter-osprey/`) | `Cargo.toml` | Rust | cargo |
| `src/toolchain/vscode-extension/` | `package.json` | TypeScript | npm |
| `src/toolchain/webcompiler/` | `package.json` | Node.js | npm |
| `src/toolchain/website/` | `package.json` | Node.js | npm |

Process each one in order.

## Step 2 — List outdated packages

Run the appropriate command to list what's outdated BEFORE upgrading anything.

### Rust (`src/toolchain` workspace)
```bash
cargo update --dry-run --verbose
```
**Read the docs:** https://doc.rust-lang.org/cargo/commands/cargo-update.html

### Node.js/npm (`src/toolchain/vscode-extension/`, `src/toolchain/webcompiler/`, `src/toolchain/website/`)
```bash
cd src/toolchain/vscode-extension && npm outdated
cd src/toolchain/webcompiler && npm outdated
cd src/toolchain/website && npm outdated
```
**Read the docs:** https://docs.npmjs.com/cli/v10/commands/npm-update

If `--check-only` was passed, **stop here** and report the outdated list.

## Step 3 — Read the official upgrade docs

**Before running any upgrade command, you MUST fetch and read the official documentation URL listed above for the detected package manager.** Use WebFetch to retrieve the page. This ensures you use the correct flags and understand the behavior.

## Step 4 — Upgrade packages

### Rust (`src/toolchain` workspace)
```bash
cargo update          # semver-compatible (within Cargo.toml ranges)
```
For a specific package: `cargo update -p <crate>`. With `--major`, edit the
version requirement in the relevant `Cargo.toml` first, then `cargo update`.

### Node.js (npm)
```bash
npm update                            # semver-compatible (within package.json ranges)
# --major flag:
npx npm-check-updates -u && npm install   # bump package.json to latest majors
```
Run in each of: `src/toolchain/vscode-extension/`, `src/toolchain/webcompiler/`, `src/toolchain/website/`.

## Step 5 — Verify the upgrade

After upgrading, run the project's build and test suite to confirm nothing broke:

```bash
make ci
```

If tests fail:
1. Read the failure output carefully
2. Check the changelog / migration guide for the upgraded packages
3. Fix breaking changes in the code
4. Re-run tests
5. If stuck after 3 attempts on the same failure, report it to the user with the error details and the package that caused it

## Step 6 — Report

Provide a summary:

- Packages upgraded (old version -> new version)
- Packages skipped (and why)
- Build/test result after upgrade
- Any breaking changes that were fixed
- Any packages that could not be upgraded (with error details)

## Rules

- **Always list outdated packages first** before upgrading anything
- **Always read the official docs** for the package manager before running upgrade commands
- **Always run tests after upgrading** to catch breakage immediately
- **Never remove packages** unless they were explicitly deprecated and replaced
- **Never downgrade packages** unless rolling back a broken upgrade
- **Never modify lockfiles manually** — let the package manager regenerate them
- **Commit nothing** — leave changes in the working tree for the user to review

## Success criteria

- All outdated packages upgraded to latest compatible (or latest major if `--major`)
- Build passes
- Tests pass
- User has a clear summary of what changed
