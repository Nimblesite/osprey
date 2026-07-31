# Plan 0021 - Osprey GitHub Action

**Subsystem:** `tests/` (ferry round-trip), repo-root action packaging
(`action.yml`, `Dockerfile`, `main.osp`), `.github/workflows/release.yml`
**Status:** Spec written; **no implementation started.** No root `action.yml`,
no `Dockerfile` for the action image, no `main.osp`, no GHCR job in
`release.yml`, and no corpus program combines `input()` with JSON parsing. The
only `action.yml` in the tree is the internal
`.github/actions/setup-osprey-compiler/action.yml`, which builds the compiler
for CI and is not the marketplace action. Zero code or test references any
`[GHA-*]` spec id.
**Spec:** [0033 - Osprey GitHub Action](../specs/0033-OspreyGitHubAction.md)
(`[GHA-*]`)

> Renumbered from `0015` on 2026-07-30: that number was already
> [plan 0015 — Generics and Variance](0015-generics-and-variance.md).

## Summary

Implement [spec 0033 - Osprey GitHub Action](../specs/0033-OspreyGitHubAction.md):
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
- The Homebrew tap already ships `osprey` ([homebrew-package/osprey.rb]).
- ~~wasm excludes `input`: `WASM_RT_SRC` (`Makefile`) has no
  `random_runtime`.~~ **Corrected 2026-07-30 — this is no longer true.**
  `WASM_RT_SRC` (`Makefile`) now lists `random_runtime`, `osp_input` is defined
  unconditionally in `compiler/runtime/random_runtime.c` (only the entropy path
  is `#if defined(__wasm__)`-guarded), and
  `tests/regressions/basics/math/comprehensive_math.test.osp` calls `input()`
  while staying out of `tests/WASM_UNPORTABLE.txt` — so it runs under wasm32 too.
  Phase 5 is therefore unblocked on the runtime side; what it still lacks is the
  WASI host shim, not a runtime change.
- **The harness cannot feed stdin.** `crates/run_test_corpus.sh` invokes
  `$BIN "$file" --run --quiet --memory=…` with no `<` redirect and no per-file
  `.stdin` fixture convention, so Phase 1 must either add that convention to the
  shared invocation for all 160 programs or prove the round-trip from a
  `cli_e2e` Rust test that pipes stdin itself. Decide this before editing the
  corpus script.

## Phase 1 — Ferry round-trip example [GHA-IO-CONTRACT]

Prove the mechanism before wrapping any YAML around it.

- Add one `tests/regressions/` program that calls `input()` once, parses the JSON
  line with the string builtins ([BUILTIN-STRING-*]), computes a result, and
  prints `key=value` lines as a single interpolated string.
- Provide `.expectedoutput`; register the twin so `crates/run_test_corpus.sh`
  checks it byte-for-byte. Feed the JSON line by piping the fixture into
  `osprey … --run` in the harness invocation.
- Prefer expanding an existing string/JSON example over adding a new file if one
  already exercises `input()` + parsing; only add a file if none fits.
- Reference `[GHA-FERRY-STDIN]` and `[GHA-IO-CONTRACT]` in the program's header
  comment so `spec-check` links it.

Exit criterion: `make test` green with the round-trip covered.

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

- Composite `action.yml`: install `osprey` (Homebrew/Scoop), ensure `clang`,
  then `printf '%s' "$INPUT_PAYLOAD" | osprey "$GITHUB_ACTION_PATH/main.osp"
  --run >> "$GITHUB_OUTPUT"` with `INPUT_PAYLOAD: ${{ toJSON(inputs) }}`.
- Cache the toolchain install across runs.

Exit criterion: same output as Phase 2 on `ubuntu`, `macos`, and `windows`
runners.

## Phase 4 — Marketplace [GHA-MARKETPLACE]

- Root `action.yml` metadata, `branding`, README with usage, versioned via the
  tag (no hard-coded version — Shipwright placeholder rule).
- Publish to the Marketplace on a `vX.Y.Z` tag.

## Phase 5 — wasm packaging (deferred) [GHA-WASM-DEFERRED]

Adopt the zero-toolchain distribution story. Both paths' *runtime*
prerequisites already landed; what remains is the host shim. Either suffices:

1. ~~Add `input` to the wasm runtime~~ — **already done**: `WASM_RT_SRC` lists
   `random_runtime` and the excluded-symbol golden harness is green with it. What
   is left on this path is shipping a committed `main.wasm` run by
   `wasmtime`/Node WASI with the JSON line on stdin.
2. Or ferry via the browser host ABI ([WASM-WEB-ABI]): the export plumbing exists
   (`WEB_DISPATCH` / `web_dispatch_export` / `with_web_dispatch_thunk` in
   `crates/osprey-cli/src/wasm.rs`, the bridge in `compiler/runtime/web_runtime.c`,
   `osp_alloc` in the memory backends). Missing: the Node WASI host shim that
   allocates the payload, writes it into linear memory, and calls
   `osprey_web_dispatch(message)`.

Exit criterion: the same round-trip passes with only a WASI host on the runner —
no `osprey`, `clang`, or runtime archive installed.

## Non-Goals

- No network from inside the program on the wasm path — sockets/HTTP are outside
  the portable subset ([WASM-TARGET]); network stays in the YAML/JS shell.
- No positional multi-line stdin protocol — one JSON line only
  ([GHA-FERRY-STDIN]).
- No hard-coded version fields anywhere in source.

## TODO

Nothing is implemented. Every item below is unstarted; the evidence for each is
in §Status and §Verified Constraints.

Phase 1 — ferry round-trip [GHA-IO-CONTRACT]:

- [ ] Decide how a program under test receives stdin: a `.stdin` fixture
      convention in `crates/run_test_corpus.sh` (which today pipes nothing to any
      of its 160 programs) **or** a `cli_e2e` Rust test that pipes the JSON line
      itself. This gates every other Phase 1 item.
- [ ] One program calling `input()` once, parsing the JSON line with
      [BUILTIN-STRING-*], and printing `key=value` as a single interpolated
      string. Expand
      `tests/regressions/basics/math/comprehensive_math.test.osp` (already calls
      `input()`; its golden line `stdin=[]` changes once stdin is fed) or
      `tests/regressions/basics/json/json_document_query.test.osp` (parses JSON,
      never reads stdin) rather than adding a file.
- [ ] `.expectedoutput` golden for it, plus the `.ospml` twin the paired corpus
      requires; green under default / `--memory=gc` / `--memory=arc`.
- [ ] Cite `[GHA-FERRY-STDIN]` and `[GHA-IO-CONTRACT]` in the program header —
      today **no** code or test references any `[GHA-*]` id, so `spec-check`
      links nothing.

Phase 2 — Docker packaging [GHA-PACKAGING-DOCKER]:

- [ ] `main.osp` at a fixed path (no `main.osp` exists anywhere in the tree).
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
      `clang`, then ferrying `INPUT_PAYLOAD: ${{ toJSON(inputs) }}` into
      `osprey main.osp --run >> "$GITHUB_OUTPUT"`.
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
- [ ] Node WASI host shim: allocate the payload with `osp_alloc`, write it into
      linear memory, call the exported `osprey_web_dispatch(message)` — or run a
      committed `main.wasm` under `wasmtime`/Node WASI with the JSON line on
      stdin. The export plumbing and bridge already exist.
- [ ] Round-trip green with only a WASI host on the runner — no `osprey`,
      `clang`, or runtime archive installed.

- [ ] `make ci` green.
