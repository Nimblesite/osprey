# Static branch review: test/spec-enforcement-coverage

Date: 2026-10-25. Verdict: **changes required**.

## Scope and evidence

Reviewed the branch first observed when this review started: `test/spec-enforcement-coverage` at `b0bb815d52c91ae423c6c21f67ad0a108b6e70b3`, against `origin/main` at `9de6122762a06da7af0655c3ffc3c9d1d13d20c3`. The comparison contains 686 changed files, 82,552 insertions and 2,004 deletions. The checkout changed during the review, so source inspection used an immutable archive of that branch commit. This report does not claim to review the subsequently selected branch or its uncommitted changes.

This was a static review of diffs, implementation, test discovery, assertions, fixtures, build wiring and changed completion claims. No CI, tests, compiler executions, builds, formatters or generators were run. Findings below follow directly from the inspected source; the proposed regression cases have not been executed. Given the size of the branch, this is not an exhaustive verification of every new package algorithm or every specification requirement. The pinned lspkit implementation and SharpLsp checkout were not independently audited.

Source links below use the reviewed commit because the active checkout can differ.

## Findings

### 1. [P1] Capability confinement can be bypassed through interpolation

Location: [`registry/core/src/surface.osp:102`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/registry/core/src/surface.osp#L102).

The scanner enters `stringMode` on a double quote and stays there until another quote or escape. It never returns to code mode for interpolation. For example, the executable `readFile` identifier in `fn load(path) = "${readFile(path)}"` is discarded as string content. The resulting surface omits `io/files`, so the resolver can admit the dependency without that grant. Installation and verification repeat the same scanner, so they cannot detect the omission later.

This contradicts `[PACKAGE-CAPABILITY-CONFINEMENT]`, which currently makes the scan the enforcement boundary until compiler enforcement exists. `surface_tests.osp` tests inert strings and ordinary calls, but no executable interpolation.

Required: derive capabilities from parsed executable expressions, including interpolation in every supported flavor, or reject forms the scanner cannot safely inspect. Add publication-to-lock tests that require the grant for an interpolated ambient call. The completed capability-surface item in plan 0020 is not established by the current tests.

### 2. [P1] The new mandatory package tests cannot pass with the committed package inventory

Locations: [`crates/osprey-cli/tests/python_packages.rs:64`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/crates/osprey-cli/tests/python_packages.rs#L64) and [`:258`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/crates/osprey-cli/tests/python_packages.rs#L258).

The integration test discovers every `packages/*/tests/osprey.toml`, including unfinished ports. Seven discovered packages have neither `tests/expected_output.txt` nor a package `README.md`: **einops, jinja2, numpy, re, safetensors, scipy, and torch**.

For each of those seven directories, the missing golden becomes an empty string, and `golden.is_empty() || stdout != golden` necessarily reports failure regardless of program output. The independent publishability test also necessarily fails its README assertion. Marking a catalog row “in progress” does not exclude it from either test. These tests are ordinary Rust integration tests, so they enter the existing workspace test run.

Required: finish the discovered suites with independently generated oracle values, committed goldens and documented API limits. Do not skip the suites or relax the mandatory assertions to obtain a green result. This is a statically demonstrable gate failure, not a claim that tests were run.

### 3. [P1] Python dependency installation ignores the Python lock pins before invoking uv

Locations: [`registry/cli/lib/install.osp:143`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/registry/cli/lib/install.osp#L143) and [`registry/cli/lib/python_plane.osp:234`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/registry/cli/lib/python_plane.osp#L234).

`Install::synced` receives the pinned Python plane but discards the `pins` value and calls `PythonPlane::synced(project)`. That function immediately executes `uv sync --frozen`. Neither the actual generated `pyproject.toml` nor `uv.lock` is compared with the digests in `osprey.lock` on this path. Editing the generated uv lock after `lock` therefore changes what `fetch` installs without changing the reviewed Osprey lock. `--frozen` prevents re-resolution; it does not authenticate the file against Osprey's pins.

The separate verifier compares the uv lock digest, but `Install` does not invoke it before sync. Moreover, `workspaceFindings` hashes a recomposed project rather than reading and checking the actual project file uv consumes. The fixture tests check the sync command and check stale locks under `verify`; they do not prove that a tampered generated input prevents installation.

Required: check both actual generated inputs against their pins before starting uv, share that check between install and verify, and add tampered-project and tampered-lock tests asserting that no process runs. This remains a delivery gap even though the broader uv plan item is correctly unchecked.

### 4. [P1] Bare package builds resolve source paths relative to the wrong directory

Locations: [`registry/cli/lib/build.osp:62`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/registry/cli/lib/build.osp#L62) and [`:130`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/registry/cli/lib/build.osp#L130).

`Flags` defaults the project to `.`. `Build::rootPath` merely concatenates the project and source path, so a normal root `src` becomes `./src`. The derived manifest is then written under `.osprey/build/osprey.toml`, and the compiler is invoked on `.osprey/build`. Its relative paths consequently refer to `.osprey/build/src`, rather than the original project's `src`; entry paths, Python sources and relative dependency paths have the same problem. A relative `--project` also fails to establish the absolute-path assumption in the implementation comment.

The build fixtures supply an absolute project path and assert the generated text. They do not exercise the advertised default invocation against a real project. This undermines plan 0020's checked-off claim that the ordinary folder-lock build flow is complete.

Required: normalize the project directory or rebase every derived path against the derived manifest directory. Add real filesystem coverage for bare `build`, `build --run`, relative `--project`, and an explicit absolute project outside the current directory.

### 5. [P1] New device binary math functions return incorrect special values

Location: [`compiler/runtime/gpu_math_ext.metal`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/compiler/runtime/gpu_math_ext.metal#L105).

Several results follow directly from the new branches:

| Function | Input | Device result from the source | Required host-compatible result |
|---|---|---|---|
| `hypot64` | `(0, -3)` | `-3`, because the zero arm returns the other input without taking its magnitude | `3` |
| `hypot64` | `(infinity, NaN)` | NaN, because the NaN check precedes the infinity check | positive infinity |
| `pow64` | `(1, NaN)` | NaN, because the exponent check precedes the unit-base case | `1` |
| `atan264` | `(y=+0, x=-0)` | positive zero, because the both-zero arm ignores `x` | positive pi |
| `atan264` | `(y=-0, x=+0)` | negative pi | negative zero |

Relevant branches are at lines 105–109, 137–141 and 348–350. The random binary tests generate nonzero finite operands in selected exponent ranges and never enumerate these exceptional pairs. The unary special-value loop does not cover binary functions. Programs using the same kernel on the host and device can therefore disagree on signs, domain results and numeric values.

Required: add the Cartesian product of signed zeros, finite signs, infinities and NaNs for each binary function, and correct the implementations against the host contract before claiming support.

### 6. [P2] Unmapped ambient builtins are treated as harmless identifiers

Location: [`registry/core/src/surface.osp:40`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/registry/core/src/surface.osp#L40).

`capabilityOf` returns an empty capability for every identifier outside the handwritten table and the `http`/`websocket` prefixes. The compiler already exposes `termReadKey` at `crates/osprey-types/src/builtins.rs:505`, but this scanner does not classify it. A dependency can read terminal input through that builtin without an `io/terminal` grant.

The new confinement contract explicitly requires unknown builtins to become `io/unknown` and block publication until classified. The test only compares the handwritten table with another handwritten JSON table, so the same omission in both passes; it never checks the compiler's actual builtin inventory.

Required: classify all ambient terminal operations, derive or validate the inventory against compiler builtins, and fail closed on unclassified ambient operations. Test an ungranted `termReadKey` dependency and a deliberately unclassified builtin. This is separate from interpolation: ordinary direct calls already bypass the table.

### 7. [P2] New GPU parity checks accept much larger errors than their success messages claim

Location: [`compiler/runtime/gpu_shader_tests.c:249`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/compiler/runtime/gpu_shader_tests.c#L249).

The added unary `check_soft` allows up to **16 ulps**, and the added binary `soft_binary_case` allows up to **64 ulps**. Nevertheless, `check_soft_binary` prints “binary within 1 ulp”. The older `check_transcendental` actually enforces one ulp and at least 99% exact agreement. These new helpers do not establish an equivalent numerical guarantee, and large trial counts do not repair the weaker acceptance rule or the omitted exceptional pairs in finding 5.

There is also vacuous new coverage at lines 186–189: the purported `floatBits` check compares `specials[i]` against a conditional whose two branches are both `specials[i]`; the `floatFromBits` check compares the word with itself. Neither calls the device emitter or verifies a conversion.

Required: make assertions establish the advertised numerical contract, correct misleading success text, and replace identity assertions with emitted-kernel round trips over signed zeros, infinities and NaN payloads. This is inadequate new coverage; the inspected diff does not remove the existing strict exp/ln/tanh checks.

### 8. [P2] The deterministic Hunt regression gate accepts an unexplored or incomplete clean report

Locations: [`scripts/hunt_expect.py:43`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/scripts/hunt_expect.py#L43) and [`scripts/make/tests.mk:203`](https://github.com/Nimblesite/osprey/blob/b0bb815d52c91ae423c6c21f67ad0a108b6e70b3/scripts/make/tests.mk#L203).

The recipe discards Hunt's exit status with `|| true`. The expectation checker then compares only the set of findings. For the three fixtures whose expectation is `none`, a report with zero completed paths, zero findings and solver/budget incompleteness still prints `ok`. For example, `{"findings":[],"explored":{"paths":0},"incomplete":[{"reason":"solver timeout"}]}` satisfies that comparison. A regression that stops exploration before the repaired code executes can therefore pass the new regression gate.

The gate also skips successfully when KLEE or its runtime bitcode is absent; the inspected CI workflows do not install KLEE. Mock tests remain useful, but do not execute the real engine or establish end-to-end replay. The deterministic recipe explicitly uses `--no-replay`.

Required: for these small deterministic fixtures, require a valid report, expected process status, nonzero completed exploration and no unexpected incompleteness. Preserve separately documented budget-limited corpus behavior. Add a real-engine/replay check in an environment with the toolchain before treating Phase 3's integration proof as complete.

## Completion claims and coverage assessment

| Plan or contract | Static assessment |
|---|---|
| Plan 0020: completed capability confinement and C1–C8 client hardening | **Not proven.** Findings 1, 4 and 6 contradict the checked-off guarantees. The fixture suites omit the relevant adversarial inputs and default build path. |
| Plan 0020: folder-lock client and build flow | Implementation and fixtures exist, but the absolute-path fixtures do not prove the normal default command. Registry suites are explicitly excluded from `make test` and CI; the plan honestly leaves CI integration unchecked. A green ordinary branch check cannot prove this subsystem. |
| Plan 0020: resolver property testing | Generated graphs and an exhaustive small-graph oracle exist. This is meaningful bounded evidence, not proof over arbitrary catalogs. The 100,000-release performance gate, signed time input, trust plane and durable transactions remain unchecked; this review does not relabel them completed. |
| Plan 0034: symbolic execution Phases 1–3 delivered | Mock, codegen and fake-engine tests exist. Real-engine regression assertions are insufficient as described in finding 8, and native replay is excluded from that recipe. The plan's opening says the corpus is not green, while later sections say it is green; reconcile the status with one dated, retained run artifact. No such execution was repeated in this review. |
| Plan 0034: Phases 4–6 | Correctly remain open: typed harness, libkleeRuntest replay, regression generation, required corpus check, Windows and comparative proof benchmarks. Existing native stdin/file replay does not satisfy the deferred libkleeRuntest work. |
| Python port catalog and `[PORT-SUITE]` | Shipped rows have committed golden files in the inspected inventory, but the mandatory discovery also includes seven unfinished suites lacking their required artifacts. No all-backend pass is established by this review. The export check counts a name appearing anywhere in suite text; that is an inventory check, not proof of a call or a behavioral assertion for every export. |
| Shared profiler integration | The plan records completed UI work plus a remaining upstream merge/repin. Its Windows sampler and heap backend are explicitly open in plan 0035. Shared-library internals and the other host were not independently reviewed, so their completion claims are not certified here. |

## Test weakening and gate changes

The inspected assertion removals in the ABI, ML spawn-lowering and shared type-check helpers are refactors into shared assertions, rather than demonstrated lost checks. `coverage-thresholds.json` is unchanged in this comparison. The release workflow adds an unconditional vulnerability audit and dependencies from publishing jobs; that is a stronger gate on the face of the static wiring. This review does not verify live GitHub rulesets, scanner results or workflow execution.

The concrete coverage problems are the guaranteed failures in finding 2, the weaker new numerical checks in finding 7, the incomplete-run acceptance in finding 8, and missing adversarial/integration cases alongside findings 1–6. Do not close the affected plan items based solely on spec-ID comments, fixture counts, a copied builtin table or passing mocks. Keep them partial until tests assert the missing behavior and retained execution evidence shows the relevant integration checks passed.

No implementation, tests, thresholds or plans were modified for this review.
