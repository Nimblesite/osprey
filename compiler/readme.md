# Osprey Compiler Assets

The compiler itself is the Rust workspace at the repository root
([crates/](../crates/), binary `osprey`). This directory holds the C runtime
that compiled programs link against:

- [runtime/](runtime/) - The C runtime (fibers, effects, HTTP/WebSocket,
  strings, lists, maps, JSON, terminal). Built into `lib/libfiber_runtime.a` /
  `lib/libhttp_runtime.a` by `make _runtime`; `osprey` links these at
  `--compile`/`--run` time.

Example programs and the golden test suites live at the top-level
[examples/](../examples/) directory: [tests/regressions/](../tests/regressions/)
(golden programs run in CI against `.expectedoutput` via
[crates/run_test_corpus.sh](../crates/run_test_corpus.sh)),
[examples/failscompilation/](../examples/failscompilation/) (the must-reject
ratchet), and [examples/tui/](../examples/tui/) (the `make _tui` showcase).

## Related Projects

- [VSCode Extension](../vscode-extension/) - Language support for VS Code
- [Web Compiler](../webcompiler/) - Browser-based playground
- [Website](../website/) - Official documentation site

## Development Notes

- Don't put .osp files in the root of this folder
- This is for the C runtime only - no loose project files
