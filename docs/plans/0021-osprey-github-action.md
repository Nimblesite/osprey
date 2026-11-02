# Plan 0021 - Osprey GitHub Action

**Subsystem:** `tests/` (ferry round-trip), repo-root action packaging
(`action.yml`, `Dockerfile`, `main.osp`), `.github/workflows/release.yml`
**Status:** Phase 1 done: the stdin ferry and exit status are tested. No root `action.yml`,
no `Dockerfile` for the action image, no `main.osp` and no GHCR job in
`release.yml`. The
only `action.yml` in the tree is the internal
`.github/actions/setup-osprey-compiler/action.yml`, which builds the compiler
for CI and is not the marketplace action.
**Spec:** [0033 - Osprey GitHub Action](../specs/ecosystem/0033-OspreyGitHubAction.md)
(`[GHA-*]`)

> Renumbered from `0015` on 2026-07-30: that number was already
> plan 0015 — Generics and Variance (now retired; [shipped contract](../specs/language/0004-TypeSystem.md#generics-and-variance)).

## Summary

Implement [spec 0033 - Osprey GitHub Action](../specs/ecosystem/0033-OspreyGitHubAction.md):
a marketplace action that is a thin wrapper over a co-located `.osp` program.
The wrapper serializes a job's inputs into one JSON line, feeds it to the native
Osprey program on stdin, and routes the program's `key=value` stdout to
`$GITHUB_OUTPUT`.

The target is **native**, not wasm — but that is now a *packaging* choice, not a
forced one. See the corrected constraint below: `input` reaches wasm today.

## Verified Constraints

These were checked against the tree before planning. Two have since gone stale
and are corrected here rather than left to mislead.

- `--run` executes the binary with inherited stdio and **no argv forwarding**
  (`execute_native`, `Command::new(&exe).status()`).
- No environment-variable builtin exists — an `.osp` program cannot read
  `INPUT_*`.
- `input` ([BUILTIN-INPUT], `osp_input` in `random_runtime.c`) reads **one line**
  from stdin, strips the newline, returns `""` on EOF — the sole input channel.
- Native `--run` requires `clang` (`OSPREY_CC`) plus a `libfiber_runtime*.a`
  archive on the runner.
- The Homebrew tap already ships `osprey` ([the release process](../RELEASING.md)).
- ~~wasm excludes `input`: `WASM_RT_SRC` (`Makefile`) has no
  `random_runtime`.~~ **Corrected 2026-07-30 — this is no longer true.**
  `WASM_RT_SRC` (`Makefile`) now lists `random_runtime`, `osp_input` is defined
  unconditionally in `compiler/runtime/random_runtime.c` (only the entropy path
  is `#if defined(__wasm__)`-guarded), and
  `tests/regressions/basics/math/comprehensive_math.test.osp` calls `input()`
  while staying out of `tests/WASM_UNPORTABLE.txt` — so it runs under wasm32 too.
  Phase 5 is therefore unblocked on the runtime side; what it still lacks is the
  WASI host shim, not a runtime change.
- **The harness has no per-program stdin fixtures.** `crates/run_test_corpus.sh` invokes
  `$BIN "$file" --run --quiet --memory=…` with stdin redirected from `/dev/null` and no per-file
  `.stdin` fixture convention, so Phase 1 must either add that convention to the
  shared invocation for all corpus programs or prove the round-trip from a
  `cli_e2e` Rust test that pipes stdin itself. Decide this before editing the
  corpus script.

## Phase 1 — Ferry round-trip example [GHA-IO-CONTRACT]

Prove the mechanism before wrapping any YAML around it.

- Add one `tests/regressions/` program that calls `input()` once, parses the JSON
  line with the JSON builtins ([BUILTIN-JSON]), computes a result, and
  prints `key=value` lines as a single interpolated string.
- Provide `.expectedoutput`; register the twin so `crates/run_test_corpus.sh`
  checks it byte-for-byte. Feed the JSON line by piping the fixture into
  `osprey … --run` in the harness invocation.
- Prefer expanding an existing string/JSON example over adding a new file if one
  already exercises `input()` + parsing; only add a file if none fits.
- Reference `[GHA-FERRY-STDIN]` and `[GHA-IO-CONTRACT]` in the program's header
  comment so `spec-check` links it.

- Establish an explicit failure-status mechanism: ordinary `main` results are currently discarded by `crates/osprey-codegen/src/lower.rs`. A returned `Error` or integer alone cannot fail the action. Pin valid input, invalid JSON, application failure, and wrapper pipeline failure before accepting packaging.

Exit criterion: `make test` green with the round-trip and failure-status behavior covered.

## Phase 2 — Docker packaging [GHA-PACKAGING-DOCKER]

The recommended, hermetic packaging.

- `Dockerfile`: base image with `clang`, the built `osprey` binary, the runtime
  archives, and `main.osp` at a fixed path. Optionally pre-compile `main.osp` to
  a native binary in the image build so container start skips codegen.
- Entrypoint reads the ferried JSON line on stdin and runs the program (or the
  pre-built binary), passing stdin through unchanged; stdout is the action
  output.
- `action.yml` (`runs.using: docker`, image pinned by GHCR digest) with typed
  `inputs`/`outputs` and `branding`.
- Build/publish the image from the release pipeline; pin by `sha256` digest.

Exit criterion: a workflow consuming the local action reproduces the Phase 1
output for the same inputs.

## Phase 3 — Composite packaging [GHA-PACKAGING-COMPOSITE]

The cross-OS alternative, sharing the same `main.osp`.

- Composite `action.yml`: install `osprey` (Homebrew/Scoop), ensure `clang` and `jq`,
  then `set -euo pipefail` and `printf '%s' "$INPUT_PAYLOAD" | jq -c . | osprey "$GITHUB_ACTION_PATH/main.osp"
  --run >> "$GITHUB_OUTPUT"` with `INPUT_PAYLOAD: ${{ toJSON(inputs) }}`.
- Cache the toolchain install across runs.

Exit criterion: same output as Phase 2 on `ubuntu`, `macos`, and `windows`
runners.

## Phase 4 — Marketplace [GHA-MARKETPLACE]

- Root `action.yml` metadata, `branding`, README with usage, versioned via the
  tag (no hard-coded version — Shipwright placeholder rule).
- Publish to the Marketplace on a `vX.Y.Z` tag.

## Phase 5 — wasm packaging (deferred) [GHA-WASM-DEFERRED]

Distribute a precompiled `main.wasm` and invoke it under wasmtime or a Node WASI host with the compact JSON line on stdin. `WASM_RT_SRC` already includes `random_runtime`; runtime support for `input` is not a blocker. Delivery still requires packaging, stdout forwarding, and explicit failure-status tests.

The existing browser ABI ([WASM-WEB-ABI]) could support a separate message-based design, but calling `osprey_web_dispatch(message)` directly does not satisfy this plan's stdin-only ferry contract. Such a design would require a separately specified contract and acceptance tests.

Exit criterion: the same round-trip passes with only a WASI host on the runner —
no `osprey`, `clang`, or runtime archive installed.

## Non-Goals

- No network from inside the program on the wasm path — sockets/HTTP are outside
  the portable subset ([WASM-TARGET]); network stays in the YAML/JS shell.
- No positional multi-line stdin protocol — one JSON line only
  ([GHA-FERRY-STDIN]).
- No hard-coded version fields anywhere in source.

## TODO

Phase 1 is done; the packaging phases are unstarted.

Phase 1 — ferry round-trip [GHA-IO-CONTRACT]:

- [x] A program under test receives stdin from a `cli_e2e` test that pipes the JSON
      line itself, since the corpus harness feeds no stdin.
- [x] Application failure status: an integer returned from `main` is the exit status
      ([ENTRY-EXIT-STATUS]). Valid input, invalid JSON, non-object JSON and application
      errors are each checked; wrapper pipeline failure belongs to the packaging phases.
- [x] One program calls `input()` once, parses the line and prints `key=value` lines as
      one interpolated string: `a_json_line_on_stdin_becomes_outputs_and_an_exit_status`.
- [x] The test cites `[GHA-FERRY-STDIN]` and `[GHA-IO-CONTRACT]`.

The phases below need two owner decisions first: what the published action computes,
since the spec fixes only its shape, and the GHCR and Marketplace publication.

Phase 2 — Docker packaging [GHA-PACKAGING-DOCKER]:

- [ ] `main.osp` at a fixed path (no action `main.osp` exists at the repository root).
- [ ] Action `Dockerfile` with `clang`, the `osprey` binary and the runtime
      archives, optionally pre-compiling `main.osp` at image build. The two
      existing Dockerfiles (`webcompiler/`, `.devcontainer/`) are unrelated.
- [ ] Entrypoint that passes the ferried stdin line through unchanged and emits
      the program's stdout as the action output.
- [ ] Root `action.yml` (`runs.using: docker`, GHCR image pinned by `sha256`
      digest) with typed `inputs`/`outputs` and `branding`.
- [ ] GHCR build/publish job in `.github/workflows/release.yml`, which currently
      has no docker job at all.

Phase 3 — Composite packaging [GHA-PACKAGING-COMPOSITE]:

- [ ] Composite `action.yml` installing `osprey` from the tap / Scoop, ensuring
      `clang`, then ferrying `INPUT_PAYLOAD: ${{ toJSON(inputs) }}` through
      a `jq -c .` compaction stage and `osprey main.osp --run >> "$GITHUB_OUTPUT"`, with `pipefail` enabled so malformed input fails the step.
- [ ] Toolchain install cached across runs (caching exists only for the internal
      `setup-osprey-compiler` action's `~/.cargo`).
- [ ] Same output as Phase 2 on `ubuntu`, `macos` and `windows` runners.

Phase 4 — Marketplace [GHA-MARKETPLACE]:

- [ ] Root `action.yml` metadata + `branding` + a README usage section, version
      stamped from the tag (never hard-coded — [SWR-VERSION-*]).
- [ ] Marketplace publication step on a `vX.Y.Z` tag; `release.yml` publishes
      binaries, the GitHub Release, the tap, Scoop and the VSIX, but no action.

Phase 5 — wasm packaging [GHA-WASM-DEFERRED]:

- [x] `input` available on wasm32 — `WASM_RT_SRC` includes `random_runtime`, and
      a corpus program calling `input()` runs under wasm32 without a
      `WASM_UNPORTABLE` exclusion. This item was listed as the phase's blocker;
      it is no longer one.
- [ ] Package a precompiled `main.wasm` under wasmtime or Node WASI, supplying the compact JSON line on stdin, forwarding stdout, and preserving failure status.
- [ ] Round-trip green with only a WASI host on the runner — no `osprey`,
      `clang`, or runtime archive installed.

- [ ] `make ci` green.
