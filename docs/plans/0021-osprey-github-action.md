# Plan 0021 — Osprey GitHub Action

Contract: ../specs/ecosystem/0033-OspreyGitHubAction.md

## Prerequisites in place

- Ferry + exit status proven: `a_json_line_on_stdin_becomes_outputs_and_an_exit_status` (crates/osprey-cli/tests/cli_e2e.rs) pipes one JSON line into `--run` and asserts `key=value` stdout and exit codes for valid, invalid, non-object, application-error and EOF inputs.
- `input` ships in the wasm runtime: `WASM_RT_SRC` (scripts/make/config.mk) includes `random_runtime`; `tests/regressions/basics/math/comprehensive_math.test.osp` calls `input()` and is absent from `tests/WASM_UNPORTABLE.txt`.

## Constraints

- `osprey <file> --run` inherits stdio and forwards no argv (`execute_native` in crates/osprey-cli/src/native.rs uses `Command::new(&exe).status()`); no env builtin exists, so stdin `input` is the only ferry channel.
- One JSON line only; no argv/env ferry ([GHA-FERRY-STDIN]).
- Fail the action by returning a non-zero int from `main`; a returned `Error` does not fail it ([ENTRY-EXIT-STATUS]).
- No hard-coded version; the tag stamps it ([SWR-VERSION-BUILD-STAMPING]).
- Docker actions run on Linux runners only ([GHA-PACKAGING-DOCKER]).
- Every job added to `.github/workflows/release.yml` must be listed in `release-complete.needs` and its credential checked in `preflight` (`node scripts/verify-release-gates.mjs` runs in `make ci` and fails otherwise).
- Owner decisions needed before Phase 2/4: what the published action computes (the spec fixes only its shape) and the GHCR org + Marketplace listing.

## Phase 2 — Docker packaging [GHA-PACKAGING-DOCKER]

- [ ] Create `main.osp` at the repository root (co-located with `action.yml`, [GHA-WRAPPER]): one `input()` call, parse the JSON line with [BUILTIN-JSON], print `key=value` lines as a single interpolated string, return a non-zero int on failure. Check: `osprey main.osp --run < payload.json` reproduces the cli_e2e ferry stdout and exit code for the same JSON lines.
- [ ] Add a root `Dockerfile` baking `clang`, the `osprey` binary, the `libfiber_runtime*.a` archives, and `main.osp` at a fixed path; optionally pre-compile `main.osp` to a native binary. (`webcompiler/Dockerfile` and `.devcontainer/Dockerfile` are unrelated.)
- [ ] Add the container entrypoint: pass the ferried stdin line through unchanged and route the program's stdout to `$GITHUB_OUTPUT`.
- [ ] Root `action.yml`: `runs.using: docker`, image pinned by GHCR `sha256` digest, typed `inputs`/`outputs`.
- [ ] Add a GHCR build/publish job to `.github/workflows/release.yml`, wired into `release-complete.needs` and `preflight`. Check: `node scripts/verify-release-gates.mjs` passes and a consuming workflow reproduces the Phase 1 output for the same inputs.

## Phase 3 — Composite packaging [GHA-PACKAGING-COMPOSITE]

- [ ] Composite `action.yml` (cross-OS alternative, same `main.osp`): install `osprey` (Homebrew tap / Scoop), ensure `clang` and `jq`, then run `set -euo pipefail`; `printf '%s' "$INPUT_PAYLOAD" | jq -c . | osprey "$GITHUB_ACTION_PATH/main.osp" --run >> "$GITHUB_OUTPUT"` with `INPUT_PAYLOAD: ${{ toJSON(inputs) }}`.
- [ ] Cache the toolchain install across runs.
- [ ] Prove the same output as Phase 2 on `ubuntu`, `macos` and `windows` runners.

## Phase 4 — Marketplace [GHA-MARKETPLACE]

- [ ] Root `action.yml` metadata (`name`, `description`, `author`, `branding`) plus a README usage section; version stamped from the tag.
- [ ] Marketplace publication step on a `vX.Y.Z` tag (`release.yml` currently publishes binaries, the GitHub Release, tap, Scoop and VSIX — no action).

## Phase 5 — wasm packaging [GHA-WASM-DEFERRED]

- [ ] Package a precompiled `main.wasm` under wasmtime or a Node WASI host: supply the compact JSON line on stdin, forward stdout, preserve failure status.
- [ ] Round-trip green with only a WASI host on the runner — no `osprey`, `clang`, or runtime archive installed.

- [ ] `make ci` green.
