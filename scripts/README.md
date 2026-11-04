# Development tools

Run commands from the repository root. The root Makefile is the build entry point; do not invoke individual fragments as standalone makefiles.

| Directory or files | Purpose |
| --- | --- |
| `make/` | Configuration and the private build, runtime, test, application and extension steps behind the root Makefile's targets |
| `ios.mk`, `android.mk` and platform scripts | Mobile runtime builds and acceptance tooling |
| `verify-*.mjs`, `test-*.py`, `ci-scope.*` | CI assertions, branch-protection checks and regression tooling |
| `wasm-*.mjs` | WASI and browser execution with byte-exact golden checks |
| `src/toolchain/schemas/` | Tooling manifest schemas |

See the [repository map](../docs/contributing/repository-map.md) for the responsibilities of each build fragment. Test artifacts belong in temporary directories or ignored build output, not beside source files.
