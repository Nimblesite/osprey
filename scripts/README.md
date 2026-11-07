# Development tools

Run commands from the repository root. The root Makefile is the build entry point; do not invoke individual fragments as standalone makefiles.

| Directory | Purpose |
| --- | --- |
| `media/` | `make media`: copies the published videos, posters and captions from the Videos checkout; commit what it copies |
| `make/` | Configuration and the private build, runtime, test, application, mobile and extension steps behind the root Makefile's targets |
| `ci/` | CI scope, gates, branch protection, root layout, dead code, docs checks and developer tool installers |
| `hunt/` | Symbolic-execution corpus runner, sampler and report, with their tests |
| `mobile/` | iOS, Android, macOS and Windows runtime builds, simulators and acceptance tooling |
| `packages/` | Package coverage, publication and license gates, the local registry and the signal-flow skin builder |
| `proof/` | OpenAI proof corpus, proof export and workbench proof staging |
| `wasm/` | WASI and browser execution with byte-exact golden checks |
| `tests/` | Language-level harnesses: Python app ports, GPT-2 flavor check, profiler and LSP benchmark |

See the [repository map](../docs/contributing/repository-map.md) for the responsibilities of each build fragment. Test artifacts belong in the build output directories, never the repository root.
