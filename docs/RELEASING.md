# Releasing Osprey

Osprey ships through a single **tag-triggered** pipeline
([`.github/workflows/release.yml`](../.github/workflows/release.yml)) built on the
[Shipwright](https://github.com/Nimblesite/Shipwright) supply-chain contract.

## The three trigger rules

| Event | What runs |
|-------|-----------|
| **Push a tag `v*`** | Resolve the tag version, audit its SHA and publish the channels selected by the release-scope classifier. |
| **Open a PR to `main`** | PR checks in [`ci.yml`](../.github/workflows/ci.yml) and [`ci-windows.yml`](../.github/workflows/ci-windows.yml); Windows also permits manual dispatch. |
| **Merge to `main`** | **Nothing.** No build, no deploy. |

## Cutting a release

```bash
# from an up-to-date main
git tag v1.2.3
git push origin v1.2.3
```

The classifier compares the tag with the newest earlier tag whose release run succeeded (a failed or cancelled release shipped nothing, so it is never a baseline) using [`.github/release-scope.json`](../.github/release-scope.json). It determines which artifacts and channels the tag requires:

| Changed surface | Release work |
| --- | --- |
| Compiler/runtime or another binary-classified path | Build the platform matrix and publish the full binary/extension channels; deploy the web compiler. |
| Extension only | Build the version-matched compiler matrix and VSIX packages; publish Marketplace and Open VSX. |
| Website/benchmarks only | Deploy the website on a stable tag; skip the compiler matrix and binary/package channels. |
| Unclassified paths | Select a full release. |

The classifier also contains explicitly ignored paths; consult the file rather than assuming every documentation edit deploys the site. A prerelease tag does not overwrite the live website.

For a full release, the matrix builds `darwin-arm64`, `linux-x64` and `win32-x64`. Each binary's version is checked before archives and SHA-256 files are uploaded to GitHub Releases. Homebrew and Scoop consume those archives; per-platform VSIX packages bundle the matching compiler and publish as `nimblesite.osprey` to Marketplace and Open VSX.

Website deployment depends on scope and the dependency audit; it does not wait for the binary publishing channels. The web compiler deployment depends on the GitHub Release and audit. `release-complete` evaluates the outcome of every required channel.

## Required release gates

- **Fresh audit of the exact tag SHA.** Every publishing/deployment path depends on the unconditional `audit` job, including website-only and prerelease runs. Scanner or network failures fail the release. `make audit` is the shared local check.
- **Required credentials.** `preflight` checks its four configured publication credentials and fails when any is missing. Builds can run in parallel with this check; it is not a dependency of the build matrix.
- **Required channels must succeed.** `release-complete` runs with `if: always()`, rejects failed/cancelled jobs, and requires success for each channel selected by scope and tag type. An expected scope-based skip is permitted; skipping a required channel fails.
- **Matrix failures remain visible.** `fail-fast: false` lets sibling build/package legs finish; it does not suppress a failing result.

[`scripts/verify-release-gates.mjs`](../scripts/verify-release-gates.mjs) checks the workflow shape during `make lint` and the required build job. Publication/deployment jobs must remain connected to `audit` and `release-complete`. The verifier's reviewed exceptions must still match their intended commands; stale exceptions fail.

## Versioning — never hard-code it ([SWR-VERSION-BUILD-STAMPING])

Source-controlled version fields MUST stay at the placeholder **`0.0.0-dev`**:

- `Cargo.toml` (`[workspace.package] version`) and the CLI fallback in
  `crates/osprey-cli/src/main.rs`
- `vscode-extension/package.json` (`version`)
- `shipwright.json` (`product.version` + each component `expectedVersion`)

The real version is stamped from the tag at build time — the `osprey` binary
via the `OSPREY_VERSION` environment variable at `cargo build` time,
`package.json`/`shipwright.json` via the release job. **A PR that changes a placeholder to a real version is a defect and
must be rejected in review.**

The compiler honors the version contract ([SWR-VERSION-CLI-OUTPUT]):

```text
$ osprey --version
osprey 1.2.3
commit 5ccded97
$ osprey --version --json
{"manifestVersion":1,"name":"osprey","version":"1.2.3","kind":"cli","product":"osprey","commit":"5ccded97"}
```

The first line stays exactly `name version`, which is what a version probe
parses. The commit comes from `OSPREY_COMMIT` (a release or clean cut) or
`GITHUB_SHA` (CI); a binary built from a working tree reports
`<sha>-worktree`, because it may contain uncommitted changes.

## Required secrets / variables

Configure these on `Nimblesite/osprey`:

| Secret | Used by | Purpose |
|--------|---------|---------|
| `BREW_SCOOP_PAT` | `brew`, `scoop` | PAT with push to `Nimblesite/homebrew-tap` and `Nimblesite/scoop-bucket`. |
| `OPEN_VSX_PAT` | `publish-openvsx` | Open VSX token for namespace `nimblesite`. |
| `AZURE_CLIENT_ID` | `publish-marketplace` | Entra app id for the Marketplace OIDC publish. Not sensitive; a secret only for convenience. |
| `AZURE_TENANT_ID` | `publish-marketplace` | Entra tenant id, likewise. |
| `FLY_API_TOKEN` | `deploy-webcompiler` | Fly.io deploy. |

The GitHub Release uses the built-in `GITHUB_TOKEN` (`contents: write`), and the
Marketplace publish mints a short-lived token from the Entra OIDC session rather
than storing a PAT.

`preflight` checks `BREW_SCOOP_PAT`, `OPEN_VSX_PAT`, `AZURE_CLIENT_ID` and `AZURE_TENANT_ID`. The web compiler deployment uses `FLY_API_TOKEN` separately. Missing credentials fail their required checks; they do not turn a publication channel into a successful skip.

## Installing released builds

```bash
# Homebrew (macOS / Linux)
brew install nimblesite/tap/osprey

# Scoop (Windows)
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket
scoop install osprey
```

Osprey invokes the native LLVM/C toolchain to compile and link generated IR. Released package-manager definitions declare these dependencies: `llvm` for Homebrew and `llvm` plus `gcc` for Scoop. See the [installation guide](../website/src/docs/installation.md) for host prerequisites.

## Windows support status

The Windows build uses MSYS2 UCRT64, MinGW/clang and winpthreads. The runtime includes Winsock2 HTTP/WebSocket adapters and a Win32 process implementation using `CreateProcessA` with stdout/stderr pipes; process spawning is no longer a stub.

[Windows CI](../.github/workflows/ci-windows.yml) builds the runtime/compiler, checks the version contract, runs fiber and feature smokes, checks caller-stack GC roots and invokes the Default/ML test tree under the three memory backends. This describes the configured checks, not a claim that every Windows runtime path has passed. [The memory contract](specs/compiler/0018-MemoryManagement.md) records the remaining Windows GC qualification; platform parity requires its own evidence.
