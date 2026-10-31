# PR 325 stabilization evidence

Reviewed on 2026-10-31. This records verified corrections on `feat/type-disciplines`; merge and release remain conditional on the latest head passing every required check.

## Corrections

- `201241f1`: reuse a checked program only for its matching source, including signed-zero distinctions, and consume a retained result even when the next compilation fails. The code-generation regression tests exercise wrong-source and stale-result boundaries.
- `47e80a76`: collect stdout and stderr independently before formatting the process demonstration. Its Default and ML syntax flavors share the unchanged post-regression golden. Deliberately split lines exercise chunk boundaries without depending on scheduler timing.
- `fa2b8103`: avoid copying a named callee's unused return model when computing only immediate effects. Preserve generic specialization, parameter names, dependency reads and diagnostic metadata. Full-state scheduler comparisons retain the established Jacobi transfer semantics, including recursive callback records.
- `d12bc825`: drain both POSIX process pipes before reporting child exit. The regression ensures the child has exited before draining, then checks all 3,072 bytes from each stream, one exit event with code 7, and no events after exit. The extracted header receives its own coverage gate; the existing gate remains enforced.
- `ea3905ca`: correct the lexical order of the installed-VSIX test's expected module inventory. The production bundle already contained the right modules. All 12 literal names, the sorted actual inventory and every per-file hash assertion remain enforced.

## Verification

The compiler at `target/summary-final-release/osprey` has SHA-256 `758cb181dced5547856997b28fba4183d0a1f27d23529649650892703b3f1b05`. A release rebuild verified the same artifact against 1,035 pinned compiler, embedded-asset and runtime source inputs with no drift. Local machine receipts are under `target/logs/ci-prep/`.

| Check | Verified result |
| --- | --- |
| `cargo test --profile ci --workspace --lib` | 1,814 passing tests across 13 groups |
| Strict workspace Clippy, formatting, Hawk and `make _deslop` | Passed |
| Independent CLI accept/reject, diagnostic and runtime controls | Separate matrices of 90, 150 and 70 passing cases |
| Native GC and ARC differential corpora | 287 matching goldens, 20 alternate GPU cases and six doctests per backend; zero ARC leaks |
| WebAssembly differential corpus | 215 matching goldens, 72 pinned unsupported cases, zero leaks, 20 alternate GPU cases and six doctests |
| Actual Workbench WASM build | Passed in 768.103 seconds; 88 source/runtime inputs unchanged; 1,389,547-byte artifact validated |
| Workbench development host, browser and visual checks | 74 passing checks against the actual final WASM; 148 source/asset inputs unchanged |
| CLI launcher integration regressions | Three passing cases, independently confirmed after the pi run |
| Process demonstration after the canonical runtime rebuild | Four matching goldens: Default/ML with GC/ARC |
| Package coverage policy regressions | 42 passing tests |
| Canonical macOS ABI and persistent globals | Passed, including GPUI exports and Storage/Counter controls |
| Native Workbench archive/host and full Swift suite | Canonical build passed; 114 XCTest cases passed with zero failures |
| Canonical package and installed production VSIX | Both commands passed; 169 cases passed on the final compiler/WASM; 1,183 input fingerprints unchanged |

The isolated analyzer and retained-cache tests are necessary to measure copies, compare otherwise inaccessible complete models and metadata, and inject retained compilation state. User-visible behavior is additionally exercised through real compiler invocations and differential goldens. Passing these checks does not establish complete CI, measured coverage for every project, Workbench visual delivery or release readiness.

## Remaining acceptance

The final Workbench WASM has SHA-256 `45f67bb9a8521b90a11a8511f24a3596e5dce427c3ef99613fdc6b1aa55301c9`. The installed test package has SHA-256 `0cfd64e65fb743ede8aa4832a6715566a1e511ba121061894b40eb67004bd7fd`. Temporary staging restored the seven original compiler/WASM/package outputs byte-for-byte. The native build regenerated its C header as expected; it matches the generated archive's header, and no other pinned native input changed. Native project manifest/entry inputs separately match the clean launch revision. These checks establish their stated local acceptance, not complete project coverage or CI on the final pushed head.

Require the remaining canonical Workbench GC/ARC suite and entry coverage probes, every project threshold and every required GitHub check on the final pushed head. Preserve the coverage producer-status guard: the earlier cancelled build left producer jobs skipped, which correctly failed that guard before any numeric coverage result existed. The older CI run also exposed the inventory-order test error corrected in `ea3905ca`; its failed result is not merge evidence for the corrected branch. Windows failures remain with their assigned owner.

After a green merge, recheck published versions and release the next stable minor using the established release workflow. Require a fresh audit of the exact tag SHA, successful publication/deployment channels, `release-complete` and verified published assets. A running build or an older green head is insufficient evidence.
