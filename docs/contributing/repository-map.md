# Repository map

Start with the component you need to change. Run build and verification commands from the repository root. `src/osprey/` holds Osprey code; `src/toolchain/` holds the Rust, C and TypeScript that build it. The Cargo workspace is `src/toolchain/Cargo.toml`, so run `cargo` from there or pass `--manifest-path`; artifacts stay in the root `target/`.

## Compiler and runtime

The compiler pipeline follows these directories in order:

| Stage | Source | Responsibility |
| --- | --- | --- |
| Parse | `src/toolchain/tree-sitter-osprey/`, `src/toolchain/crates/osprey-syntax/` | Default grammar and Default, ML and Python front ends |
| Lower | `src/toolchain/crates/osprey-ast/` | Shared AST and static interpretation |
| Check | `src/toolchain/crates/osprey-types/` | Type inference and effect requirements |
| Generate | `src/toolchain/crates/osprey-codegen/` | LLVM IR, closures, effects and GPU kernels |
| Execute | `src/toolchain/crates/osprey-cli/`, `src/toolchain/compiler/runtime/` | CLI, target linking and C runtime |

The remaining crates provide project resolution, formatting, documentation, language-server and debugger support. `src/toolchain/compiler/runtime/` contains the native and portable runtime units; `src/toolchain/compiler/bin/` and `src/toolchain/compiler/lib/` are generated outputs.

A language change starts with its normative contract in `docs/specs/language/` or `docs/specs/compiler/`, then a regression in `src/osprey/tests/` and, when appropriate, a rejection in `src/osprey/examples/failscompilation/`. Each ML twin shares its Default twin's golden. Delivery gaps belong in `docs/plans/`, not in misleading public support claims.

## Specification subjects

Core language rules live under `docs/specs/language/`; target lowering, C ABI generation and debug artifacts live under `docs/specs/compiler/`. Runtime libraries, platform hosts, development tools, package infrastructure and application frameworks have separate folders. Start with the [specification subject index](../specs/README.md). Mixed contracts split along this boundary: a mobile app host is a platform contract; the compiler’s generated archive and C boundary are compiler contracts.

## Build rules

The root Makefile defines every public target, so `make` and editor task panels, which read only files named `Makefile`, list them from one place. It includes focused fragments for the configuration and the private `_` steps those targets run:

| Fragment | Owns |
| --- | --- |
| `scripts/make/config.mk` | Platform selection, paths, warning profiles and tool settings |
| `scripts/make/runtime.mk` | Native and WASI runtime archives and incremental inputs |
| `scripts/make/tests.mk` | Test runners, lint gates and coverage enforcement |
| `scripts/make/apps.mk` | Graphics hosts, GPT-2 weights, benchmark tooling and website steps |
| `scripts/make/extension.mk` | Extension packaging and installation verification |

`scripts/make/ios.mk` and `scripts/make/android.mk` own the mobile runtime archives and test steps. Add a public target to the root Makefile and its private steps to the fragment for their responsibility; preserve its prerequisite checks. Keep `NAME := value` assignments in the fragments, because task panels list them as targets. The node dependency guard follows nested includes, so a moved recipe retains the same enforcement. Release detection classifies the shared fragments as binary inputs.

## Applications and presentation

`src/osprey/examples/` contains runnable applications and their platform hosts. `src/toolchain/website/src/docs/` contains language guides; generated reference and spec copies are ignored build output. Edit specifications under `docs/specs/`, never under `src/toolchain/website/src/spec/`.

The homepage, effects guide and handler post share their runnable draft-saving example through `src/toolchain/website/src/_data/draftExample.json`. Browser tests execute all three homepage syntax flavors and the guide/post examples, verify the displayed output and actual file contents, and assert that the offline handler writes no file. The README shows the same source and is checked against that data.

`src/osprey/packages/` is the single package tree. `manager/` contains the server, client, resolver and verification suites; its sibling directories contain libraries. Default, ML and Python flavor use the same package system. Some libraries port APIs from Python projects; their implementation syntax does not make them a different package kind. `src/toolchain/python/` contains the separate CPython interoperability bridge.

`src/toolchain/vscode-extension/` and `src/toolchain/webcompiler/` are separate Node projects with their own dependency locks. `docs/book/` and `src/toolchain/benchmarks/` contain verification and publishing tools.

## Generated files and test isolation

Each native corpus worker runs in a private directory under the harness's temporary result directory. WASI and mobile runners also isolate file writes. The harness compares the same goldens and preserves failure output and exit status. Its cleanup removes the temporary run directory when the harness exits.

Native test runs cache compiled artifacts, then execute the program again on every run. LLVM IR and object code can be shared across memory backends; each memory backend links a separate executable. Source, compiler/toolchain, build settings and runtime archive identities affect cache reuse. `OSPREY_TEST_CACHE_DIR` selects the cache directory; point it at a new private directory when reproducing a compilation or linking problem. See the [native cache contract](../specs/tooling/TestRunner.md#cli).

Manual `osprey program.osp --run` retains normal application semantics: relative paths resolve against the caller's directory. Tests executed directly, outside the corpus harness, can still leave their own files there.

## Supporting material

Official releases use `.github/workflows/release.yml` and the external Homebrew tap. `.github/` holds tooling schemas. The single review is `docs/reviews/review.md`; each finding names its revision and status.

Configuration files stay at the root when their tools discover them there. Workspace, compiler and release manifests are entry points, not miscellaneous project documentation.
