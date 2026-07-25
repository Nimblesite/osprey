# Plan 0015 - Osprey GitHub Action

## Summary

Implement [spec 0033 - Osprey GitHub Action](../specs/0033-OspreyGitHubAction.md):
a marketplace action that is a thin wrapper over a co-located `.osp` program.
The wrapper serializes a job's inputs into one JSON line, feeds it to the native
Osprey program on stdin, and routes the program's `key=value` stdout to
`$GITHUB_OUTPUT`.

The target is **native**, not wasm. This is forced, not preferred: the ferry
depends on `input` ([BUILTIN-INPUT]), and `input` is compiled out of the wasm
runtime (`WASM_RT_SRC` omits `random_runtime`). wasm packaging is deferred to a
later phase behind a runtime change.

## Verified Constraints

These were checked against the tree before planning; the plan does not re-open
them.

- `--run` executes the binary with inherited stdio and **no argv forwarding**
  (`execute_native`, `Command::new(&exe).status()`).
- No environment-variable builtin exists — an `.osp` program cannot read
  `INPUT_*`.
- `input` ([BUILTIN-INPUT], `osp_input` in `random_runtime.c`) reads **one line**
  from stdin, strips the newline, returns `""` on EOF — the sole input channel.
- Native `--run` requires `clang` (`OSPREY_CC`) plus a `libfiber_runtime*.a`
  archive on the runner.
- The Homebrew tap already ships `osprey` ([homebrew-package/osprey.rb]).
- wasm excludes `input`: `WASM_RT_SRC` (`Makefile`) has no `random_runtime`.

## Phase 1 — Ferry round-trip example [GHA-IO-CONTRACT]

Prove the mechanism before wrapping any YAML around it.

- Add one `examples/tested/` program that calls `input()` once, parses the JSON
  line with the string builtins ([BUILTIN-STRING-*]), computes a result, and
  prints `key=value` lines as a single interpolated string.
- Provide `.expectedoutput`; register the twin so `crates/diff_examples.sh`
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

Unblock and adopt the zero-toolchain distribution story. Either path suffices:

1. Add `input` to the wasm runtime: split a stdin unit out of `random_runtime`
   (or add `random_runtime`) into `WASM_RT_SRC`, keep the excluded-symbol golden
   harness green, then ship a committed `main.wasm` run by `wasmtime`/Node WASI
   with the JSON line on stdin.
2. Or ferry via the browser host ABI ([WASM-WEB-ABI]): a Node WASI host shim
   allocates the payload with `osp_alloc`, writes it into linear memory, and
   calls the exported `osprey_web_dispatch(message)`.

Exit criterion: the same round-trip passes with only a WASI host on the runner —
no `osprey`, `clang`, or runtime archive installed.

## Non-Goals

- No network from inside the program on the wasm path — sockets/HTTP are outside
  the portable subset ([WASM-TARGET]); network stays in the YAML/JS shell.
- No positional multi-line stdin protocol — one JSON line only
  ([GHA-FERRY-STDIN]).
- No hard-coded version fields anywhere in source.
