# PR 325 stabilization evidence

Reviewed on 2026-10-31. This records verified corrections on `feat/type-disciplines`; merge and release remain conditional on the latest head passing every required check.

## Corrections

- `201241f1`: reuse a checked program only for its matching source, including signed-zero distinctions, and consume a retained result even when the next compilation fails. The code-generation regression tests exercise wrong-source and stale-result boundaries.
- `47e80a76`: collect stdout and stderr independently before formatting the process demonstration. Its Default and ML syntax flavors share the unchanged post-regression golden. Deliberately split lines exercise chunk boundaries without depending on scheduler timing.
- `fa2b8103`: avoid copying a named callee's unused return model when computing only immediate effects. Preserve generic specialization, parameter names, dependency reads and diagnostic metadata. Full-state scheduler comparisons retain the established Jacobi transfer semantics, including recursive callback records.
- `d12bc825`: drain both POSIX process pipes before reporting child exit. The regression ensures the child has exited before draining, then checks all 3,072 bytes from each stream, one exit event with code 7, and no events after exit. The extracted header receives its own coverage gate; the existing gate remains enforced.
- `ea3905ca`: correct the lexical order of the installed-VSIX test's expected module inventory. The production bundle already contained the right modules. All 12 literal names, the sorted actual inventory and every per-file hash assertion remain enforced.
- `4170dbd4`: preserve callback provenance when a declared `List` or `Map` shadows the builtin type name, including generic declarations. The candidate's earlier classifier rejected valid programs; its corrected classifier distinguishes undeclared builtin names from declared owners. The permanent GC/ARC matrix executes 20 supported callback cases and checks eight exact `Alarm.ring` refusals plus two refusals of unsupported recursive dynamic calls. Existing ARC zero-live-object assertions remain enforced.
- Share immediate-effect and return-value analysis for ordinary function bodies and nested known-call arguments. The new traversal regression first failed with leaf visits `[2, 3, 5, 9]` at nesting depths `[1, 2, 4, 8]`, then passed with one visit at every depth. An independent two-walk oracle compares complete rows, returned values, dependency reads and diagnostic metadata, including populated signal, handler and field-violation maps. These comparisons preserve the original named-argument ordering and callee eligibility guards.

## Verification

The verified baseline compiler at `target/summary-final-release/osprey` has SHA-256 `758cb181dced5547856997b28fba4183d0a1f27d23529649650892703b3f1b05`. A release rebuild verified the same artifact against 1,035 pinned compiler, embedded-asset and runtime source inputs with no drift. These results predate the subsequent data-shape and combined-traversal changes; they do not establish acceptance of those changes. Local machine receipts are under `target/logs/ci-prep/`.

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

## Corrected source and release-profile acceptance

The reviewed production correction is `4170dbd4`. Its release-profile compiler at `target/combined-release/osprey` has SHA-256 `3f4b1db10d948541bb876e82cf00799290f882b1704c09c3a6a78e5a4040eb1a`; the build completed with 1,481 pinned inputs unchanged. The CI-profile artifact used for the slower local compiler and coverage checks has SHA-256 `44e9090f0179867e2f833497d28382f6e68ab778ae15cc7f012483cde9fbbb25`. Keep these artifact cohorts distinct.

| Check | Current verified result |
| --- | --- |
| Full `generic_effect_arc_result` integration suite, CI profile | 13 passing cases in 373.61 seconds; both Workbench compilation cases and the GC/ARC fault-injection case passed |
| Native differential corpus, CI profile | 287 exact goldens, 20 alternate GPU cases and six doctests under each of GC and ARC; zero ARC leaks; 1,230 input pins unchanged |
| Canonical proof-kernel Make target, CI profile | 400/400 cases and zero skips on each of GC, ARC and WASM; exact kernel/export goldens; 150.90 seconds; this is not the full Lean/speed gate |
| All package/application source coverage, CI profile | All 40 project/backend gates passed, including both mandatory Workbench entry probes; 1,593 inputs unchanged; every individual threshold retained |
| All package/application source coverage, final release profile | Independently collected and gated all 40 GC/ARC project cases with the same release compiler used in the installed package; 1,594 inputs unchanged; both full 20-project gates independently replayed; 457.69 seconds total |
| Workbench application source coverage | Both GC and ARC measured 2,078/2,280 lines (91.1%); the CI-profile GC suite plus entry completed in 598.92 seconds |
| Actual release-profile Workbench WASM build | Passed in 110.10 seconds; compiler and application inputs unchanged; full artifact bytes equal the verified `45f67bb9…` baseline |
| Browser artifact/source equivalence | The exact WASM bytes from the earlier 74 passing browser/visual checks remain unchanged, as do all 148 browser and asset inputs; no claim of a new native renderer run |
| Canonical release-profile package and installed VSIX | Both commands passed; 169 installed cases passed; 1,481 inputs unchanged and all seven staged outputs restored |
| Current strict analysis | Workspace Clippy and formatting passed; Hawk reported zero findings; the duplication gate passed |

The current installed package has SHA-256 `b00bc2c19437027d3677a0a2dd2b5cefe7715587001d99e3f3af661a9d5624ac`. Its repeated-pattern editing case passed in 21,374 milliseconds under the unchanged 30,000-millisecond deadline. An earlier installed run using the CI-profile compiler passed 168 cases and exceeded that deadline on this case; preserve that failure. The prior release baseline took 22,505 milliseconds, and standalone checks of the initial sources take approximately 8.5 milliseconds on all three compiler artifacts. Those observations do not establish the cause of the earlier editor timeout. The matched production-profile suite and the actual CI installed suite both passed without changing tests or deadlines.

On [run 41162597660](https://github.com/Nimblesite/osprey/actions/runs/41162597660), the required build, platform hosts, language corpora, Python ports, package coverage, C runtime, integration, extension, Rust coverage and full proof/Lean jobs passed on `4170dbd4`. The final coverage job's numerical gates and Apple ABI, goldens and persistence passed. Its native Workbench suite failed three of 114 cases: dialog editor focus (`FrameInheritanceTests.swift:223`), picker overflow (`MathLayoutTests.swift:62`, 304 points in a 300-point pane), and mathematical text weight (`ParagraphLayoutTests.swift:196`, 1,223.95 against 1,461.52 ±75). The run finished with failure; these assertions remain enforced. The successful extension job includes all 169 installed tests. These results must not be presented as completed required CI on a later report commit.

The existing local twelve-archive inventory is not a fully current runtime bundle: five Apple cache records are stale. The current Apple builder fingerprints every runtime header, and the release workflow invokes its phony rebuild targets before packaging. Do not promote the old cached archives or imply that their existence proves cross-platform execution.

## Remaining acceptance

The baseline Workbench WASM has SHA-256 `45f67bb9a8521b90a11a8511f24a3596e5dce427c3ef99613fdc6b1aa55301c9`. The older baseline installed package has SHA-256 `0cfd64e65fb743ede8aa4832a6715566a1e511ba121061894b40eb67004bd7fd`. Temporary staging restored the seven original compiler/WASM/package outputs byte-for-byte. The baseline native build regenerated its C header as expected; it matches that archive's header, and no other pinned native input changed. Native project manifest/entry inputs separately match the clean launch revision. These checks establish their stated baseline acceptance, not a new native build or CI on the final pushed head.

The baseline subsequently completed all 118 Workbench cases and the mandatory entry probe under both GC and ARC. Each backend measured 2,078/2,280 app lines (91.1%) and passed all 20 project gates. The collection's source-drift guard correctly failed because compiler changes landed during collection; these measurements establish baseline behavior only.

CI run `41154533373` exhausted the unchanged 40-minute Rust and package coverage budgets and the 45-minute proof budget. The missing Rust artifact then failed aggregate coverage. These are performance failures to resolve in code, not evidence that numeric thresholds should change. The later push superseded run `41160409990` on `dff4fdd7`; that run was cancelled. The reviewed shadow correction is now pushed as `4170dbd4`, whose required CI subsequently failed the three native assertions recorded above. Windows failures remain with their assigned owner.

The newly authorized removal of retired handwriting state and workspace-v1 compatibility changes application sources. All earlier Workbench application, browser, WASM, installed-package and native receipts therefore describe the preceding application cohort. The replacement workspace format must prove current title/text round-trips and explicit refusal of the retired format, preserving mathematical, lesson and file workflows. Fresh application acceptance is required before submission.

The replacement application now passes all 118 cases and its mandatory entry probe under GC and ARC, with 2,018/2,206 own-source lines covered (91.5%). The parent independently rehashed all 1,594 collector inputs with zero drift and replayed both unchanged numerical gates. Its rebuilt application WASM has SHA-256 `d6ed6f715328587bf8f9cf46418ab4d7df12b154b79ffa932edaf0f64bb4cf36`; the full browser/visual command passed 63 checks with 318 pins unchanged. Canonical packaging and the installed production VSIX passed all 169 cases with 622 pinned inputs unchanged, independently rehashed by the parent. That archive has SHA-256 `7b34742d834f63a42f8c0ee86876a8e4603efeff458636a4c6ea15a57627eb78`.

The actual WASM application suite subsequently passed all 118 cases and the entry probe against exact goldens using the canonical proof-batch WASI host and a private copied-fixture filesystem; the parent verified 1,760 unchanged inputs and both artifact hashes. Its suite artifact has SHA-256 `e2e7f2a7f53aa1de0703c4d55563a0a1ef5f44876022f16e9549ec158f126e15`. The earlier CLI-host run had no filesystem preopens and failed 39 fixture-dependent cases; that diagnostic is retained. The worker wrapper incorrectly removed its private fixture copy automatically despite the parent-only cleanup instruction; the executed wrapper and policy violation are recorded, and it must not be reused. The fresh canonical native build and all 114 Swift tests also passed locally with 1,756 inputs unchanged; the parent independently verified the archive, application and generated header hashes. Local native success does not prove the macOS 15 CI result. Run `41170562511` remains live on `7f8dbd03`; no further push is authorized before its terminal result and final batch verification.

## Mandatory completion checklist

- [x] Retrieve the failing job logs and identify the inventory error and compiler performance bottleneck.
- [x] Preserve the installed module inventory and hashes while correcting its expected lexical order.
- [x] Verify baseline native, WASM, browser, installed VSIX and GC/ARC project coverage evidence with its stated source scope.
- [x] Keep branch protection, required checks, coverage thresholds and timeouts intact.
- [x] Independently verify the declared `List`/`Map` callback correction, complete its permanent GC/ARC acceptance and refusal matrix, and push the reviewed correction. This does not replace final application coverage or CI acceptance of the performance candidate.
- [x] Complete the combined-traversal regression, semantic comparison and strict analysis checks: 1,819 workspace library tests, the additional nested metadata permutation, strict workspace Clippy, formatting and the duplication gate passed. Final application and CI acceptance remain separate.
- [x] Seal the final release compiler inputs and verify application suites, mandatory entry probes and all 40 GC/ARC coverage gates with that exact compiler. The parent rehashed all 1,594 inputs and the immutable compiler and independently replayed both complete numerical gate checks after the approved pi worker finished.
- [ ] Fix and verify all three native CI failures without weakening assertions or deadlines.
- [x] Verify the retired compatibility deletion with fresh GC/ARC/WASM application suites, coverage, actual browser, installed VSIX and local native acceptance. Final GitHub CI remains required.
- [ ] Pass every required GitHub check on the actual final pushed head and merge that head.
- [ ] Recheck release versions, publish the next stable minor after merge, and verify the exact tag audit, required channels, `release-complete` and published assets.

After a green merge, recheck published versions and release the next stable minor using the established release workflow. Require a fresh audit of the exact tag SHA, successful publication/deployment channels, `release-complete` and verified published assets. A running build or an older green head is insufficient evidence.
