# Releasing Osprey

Osprey ships through a single **tag-triggered** pipeline
([`.github/workflows/release.yml`](../.github/workflows/release.yml)) built on the
[Shipwright](https://github.com/Nimblesite/Shipwright) supply-chain contract.

## The three trigger rules

| Event | What runs |
|-------|-----------|
| **Push a tag `v*`** | The full release: build → GitHub Release → Homebrew + Scoop + Marketplace → website. |
| **Open a PR to `main`** | CI only ([`ci.yml`](../.github/workflows/ci.yml) + [`ci-windows.yml`](../.github/workflows/ci-windows.yml)). |
| **Merge to `main`** | **Nothing.** No build, no deploy. |

## Cutting a release

```bash
# from an up-to-date main
git tag v1.2.3
git push origin v1.2.3
```

That's it. The pipeline does the rest:

1. **Resolve version** — `1.2.3` is derived from the tag and validated against
   `shipwright.json`.
2. **Build** the compiler for `darwin-arm64`, `linux-x64` and `win32-x64`
   (there is no Intel-mac leg — GitHub no longer allocates the free `macos-13`
   runner), verify `osprey --version` prints `osprey 1.2.3`, and package each as
   `osprey-1.2.3-<platform>.tar.gz` (binary + every runtime archive) + `.sha256`.
3. **GitHub Release** on `Nimblesite/osprey` with all tarballs.
4. **Homebrew** — writes `Formula/osprey.rb` to
   [`Nimblesite/homebrew-tap`](https://github.com/Nimblesite/homebrew-tap).
5. **Scoop** — writes `bucket/osprey.json` to
   [`Nimblesite/scoop-bucket`](https://github.com/Nimblesite/scoop-bucket).
6. **VS Code Marketplace and Open VSX** — publish the per-platform VSIX as
   `nimblesite.osprey` (binary bundled and version-checked at activation).
7. **Website + web compiler** deploy — only after every step above succeeds.

## Any failure makes the run red

A release is either published or it is not, so every channel blocks and nothing
downgrades a failure to a warning.

- **No job is optional.** The win32 build and VSIX legs were once
  `continue-on-error`, and the web compiler deploy was non-fatal. A Windows
  binary that never built, or a playground left on a stale image, reported green
  and was found by users rather than by the pipeline.
- **A missing credential fails before anything is built.** The publish tokens
  used to be probed, and a missing one skipped its channel with a warning — a
  release that never reached Homebrew still passed. `preflight` now requires
  every one and names the ones that are absent.
- **A skipped job is a failure too.** GitHub scores a skipped job as green, so a
  wrong `if:` publishes nothing and still succeeds. The `release-complete` job
  runs `if: always()` after every other job and fails unless each channel a full
  stable release requires actually succeeded.
- **Failing does not cancel.** `fail-fast: false` stays on both matrices, so a
  broken leg still lets its siblings finish and one run shows every platform's
  result. The run just ends red.

[`scripts/verify-release-gates.mjs`](../scripts/verify-release-gates.mjs) holds
this shape in place: it runs in `make lint` and in the required "Build, Format &
Analyse" job, and fails the PR that reintroduces a swallowed failure or adds a
release job without wiring it into `release-complete`. A `|| true` that is
genuinely correct goes in that script's reviewed list with its reason; an entry
that stops matching anything fails too, so the list cannot go stale.

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
$ osprey --version --json
{"manifestVersion":1,"name":"osprey","version":"1.2.3","kind":"cli","product":"osprey"}
```

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

`preflight` checks all four publish credentials before the build starts and
fails the release naming any that are missing, so a token expiring is a red run
at minute one rather than a channel that quietly did not publish.

## Installing released builds

```bash
# Homebrew (macOS / Linux)
brew install nimblesite/tap/osprey

# Scoop (Windows)
scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket
scoop install osprey
```

Osprey shells out to LLVM (`llc`) and a C compiler (`clang`/`gcc`) at compile
time, so those are package-manager dependencies (`llvm` for brew; `llvm` + `gcc`
for scoop).

## Windows support status

The Windows build is delivered in phases (see the `[WINDOWS-PORT-*]` markers and
the C runtime under `compiler/runtime/`):

- **Phase 1 (shipped):** core language — collections, strings, fibers (via
  winpthreads), effects, pattern matching. Built under MSYS2 UCRT64.
- **Phase 2 (in progress):** HTTP / WebSocket via Winsock2.
- **Phase 3 (planned):** process spawning via the Win32 process APIs
  (`CreateProcess`); currently stubbed on Windows.
