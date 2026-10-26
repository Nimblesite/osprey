# Osprey Compiler Assets

The compiler itself is the Rust workspace at the repository root
([crates/](../crates), binary `osprey`). This directory holds the C runtime
that compiled programs link against:

- [runtime/](runtime) - The C runtime (fibers, effects, HTTP/WebSocket,
  strings, lists, maps, JSON, terminal). Built into `lib/libfiber_runtime.a` /
  `lib/libhttp_runtime.a` by `make _runtime`; `osprey` links these at
  `--compile`/`--run` time.

Runnable applications live in [examples/](../examples). Acceptance programs live in [tests/](../tests) and are compared with `.expectedoutput` files by [crates/run_test_corpus.sh](../crates/run_test_corpus.sh). [examples/failscompilation/](../examples/failscompilation) holds must-reject cases; [examples/tui/](../examples/tui) is the `make _tui` showcase.

## Related Projects

- [VSCode Extension](../vscode-extension) - Language support for VS Code
- [Web Compiler](../webcompiler) - Browser-based playground
- [Website](../website) - Official documentation site

## Development Notes

- Don't put .osp files in the root of this folder
- This is for the C runtime only - no loose project files
