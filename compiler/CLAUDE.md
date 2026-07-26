# CLAUDE.md — compiler/

The Osprey compiler itself is the Rust workspace in `crates/` (built with
`cargo`, binary `target/release/osprey`). See the root [CLAUDE.md](../CLAUDE.md)
for all build commands, architecture, and development rules.

This directory contains the C runtime that stayed behind after the Go → Rust
migration:

## Contents

- `runtime/` - Pure-C runtime libraries (fibers, HTTP/WebSocket, system, string,
  list/map, JSON, FFI, terminal). Linked by `osprey --run` as
  `libfiber_runtime.a` / `libhttp_runtime.a`.

Example programs and golden tests live at the top-level
[`../examples/`](../examples/): `tests/regressions/` (each `.osp` matches its
`.expectedoutput` in the differential harness `crates/diff_examples.sh`, run by
`make test`) and `examples/failscompilation/` (programs the compiler must reject).

## Building

The C runtime is built by the root Makefile (`make build` runs the internal
`_runtime` helper): objects and archives land in `compiler/bin/`, and the
archives are copied to `compiler/lib/`.

## C Runtime Rules

- Compiled with maximum warning levels and security hardening
  (`-D_FORTIFY_SOURCE=2`, `-fstack-protector-strong`, `-ftrapv`, `-Werror`)
- All warnings are errors; keep it that way
- HTTP/WebSocket runtime requires OpenSSL

## Example (Test) Rules

- **PREFER EXPANDING EXISTING EXAMPLES** - don't add new overlapping files
- Mix many language constructs per example; keep them concise
- **NO CONSECUTIVE PRINT CALLS** - use string interpolation
- **NO REDUNDANT TYPE ANNOTATIONS — LEAN ON INFERENCE** - Hindley-Milner
  infers parameter types, return types, and lambda parameter types. Leave them
  off. `fn add(a, b) = a + b`, never `fn add(a: int, b: int) -> int = a + b`.
  Keep an annotation ONLY when the compiler cannot infer it (empty literal with
  no context, `extern`/ambiguous return, unconstrained type variable, or a
  return type that is load-bearing for `Result` auto-unwrap). Rule of thumb: if
  deleting the annotation still compiles and the `.expectedoutput` is unchanged,
  it was redundant — delete it.
