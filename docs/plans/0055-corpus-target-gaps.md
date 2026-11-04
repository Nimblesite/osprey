# Corpus programs each target cannot run [CORPUS-TARGET-GAPS]

Contract: [WASM-TARGET-CAPABILITIES], [IOS-TARGET-CAPABILITIES], [IOS-HOST-ABI], [SYMEX-CORPUS]

Every program in `src/osprey/tests/` and `src/osprey/examples/` should run on every target. These are the ones that don't yet, grouped into the work that would make them run. The tables below were generated from the three manifests on 2026-11-05 and checked row for row.

## What the manifests are

`src/osprey/tests/WASM_UNPORTABLE.txt`, `MOBILE_UNPORTABLE.txt` and `HUNT_UNPORTABLE.txt` stay in the tree, because they are live test inputs and this plan is not a replacement for them:

- `src/toolchain/crates/run_test_corpus.sh` compares each target's rejections against the wasm and mobile files exactly, and fails on a new, removed or changed entry. An unexpected LLVM, clang or simulator failure is a test failure, never a skip. There is no regeneration flag. Deleting a file fails the run with `MISSING`, and without the pin a target could lose a capability unnoticed.
- `scripts/hunt_corpus.sh` skips the programs in the hunt file, and `scripts/test-hunt-scripts.py` reads it.

So a row leaves its manifest only when the program works on that target, and the gate forces the row out in the same commit as the fix. Wasm and mobile reasons are the compiler's own diagnostics.

## Finish

- [ ] When a manifest has no rows left, delete it along with its branch in `run_test_corpus.sh` or `hunt_corpus.sh`, and its references in specs, scripts and `.github/workflows/ci.yml`.

## `osprey hunt` (KLEE) cannot explore: `HUNT_UNPORTABLE.txt` (100 rows)

`scripts/hunt_corpus.sh` skips these unless `--all` is given. Each hunt ends on its first path with an `unmodelled` finding, because the call named has no model in the mock C library or the path reaches native code outside the module KLEE explores. Listed from the sweeps of 2026-10-24. Nothing checks this list yet (#341). Parent: [plan 0034](0034-symbolic-execution.md) [SYMEX-CORPUS].

- [ ] Model the fiber runtime for KLEE. `__osprey_coro_new`, `fiber_spawn_env_owned`, `fiber_set_deterministic_mode`, `channel_select` and `channel_create_checked` are native code outside the explored module, so every path through a resumable handler, spawn or channel ends `unmodelled`. Bring a portable fiber and channel runtime into `_runtime_klee`. Acceptance: these programs finish a hunt with no `unmodelled` finding and their rows are deleted. (59 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/examples/llms/gpt2/generate` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/examples/llms/gpt2/python/generate` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/examples/llms/gpt2/train` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/examples/handlers/returns.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/examples/handlers/returns.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/examples/handlers/semantics.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/examples/handlers/semantics.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/core/collections/nested_generic_collections_fibers.test.osp` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/core/collections/nested_generic_collections_fibers.test.ospml` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/multiplicity/multiplicity.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/multiplicity/multiplicity.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/answer_transform.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/answer_transform.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_abort_early_exit.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_abort_early_exit.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_error_policies.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_error_policies.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_lifo_audit.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_lifo_audit.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_outer_handler_bridge.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_outer_handler_bridge.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_unit_markers.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_unit_markers.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_value_rewrite.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/resume/resume_value_rewrite.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/scope/finalizers_masks_instances.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/effects/scope/finalizers_masks_instances.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/modules/ml_layout_depth.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/modules/ml_layout_depth.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/modules/module_composition.test.osp` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/modules/module_composition.test.ospml` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/modules/module_effects_rows.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/modules/module_effects_rows.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/modules/signature_ascription.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/modules/signature_ascription.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/basics/osprey_mega_showcase.test.osp` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/basics/osprey_mega_showcase.test.ospml` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/basics/types/channel_element_type_in_generic_field.test.osp` | `channel_create_checked` is native code outside the module (not in the portable runtime) |
| `src/osprey/tests/regressions/basics/website/website_examples.test.osp` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/basics/website/website_examples.test.ospml` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/abort_vs_resume.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/abort_vs_resume.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/algebraic_effects_comprehensive.test.osp` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/algebraic_effects_comprehensive.test.ospml` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/fiber_effects.test.osp` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/fiber_effects.test.ospml` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/handler_scoping.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/handler_scoping.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/retry_until_valid.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/retry_until_valid.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/staged_shared.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/effects/staged_shared.test.ospml` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/fiber/channel_select.test.osp` | fibers and continuations: `channel_select` |
| `src/osprey/tests/regressions/fiber/channel_select.test.ospml` | fibers and continuations: `channel_select` |
| `src/osprey/tests/regressions/fiber/cpu_profiling_demo.test.osp` | fibers and continuations: `__osprey_coro_new` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/fiber/fiber_determinism.test.osp` | fibers and continuations: `fiber_set_deterministic_mode` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/fiber/fiber_exact_replica.test.osp` | fibers and continuations: `fiber_set_deterministic_mode` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/fiber/fiber_showcase.test.osp` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |
| `src/osprey/tests/regressions/fiber/fiber_showcase.test.ospml` | fibers and continuations: `fiber_spawn_env_owned` is the native fiber runtime, outside the module |

- [ ] Model sockets in the mock C library. Acceptance: HTTP and WebSocket client and server programs explore past `http_create_server`, `http_create_client`, `websocket_create_server` and `osprey_net_local_address`, and their rows are deleted. (18 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/examples/statefulhttp/server.osp` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/examples/websocketserver/osprey_websocket_server.osp` | `websocket_create_server` is native code outside the module (not in the portable runtime) |
| `src/osprey/tests/regressions/effects/http_state_levels.test.osp` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/effects/http_state_levels.test.ospml` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/chunked_hostile_server.test.osp` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/chunked_hostile_server.test.ospml` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/http_client_example.test.osp` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/http_client_example.test.ospml` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/http_create_client.test.osp` | HTTP needs sockets: `http_create_client` is outside the module |
| `src/osprey/tests/regressions/http/http_create_client.test.ospml` | HTTP needs sockets: `http_create_client` is outside the module |
| `src/osprey/tests/regressions/http/http_response_handle.test.osp` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/http_response_handle.test.ospml` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/http_server_example.test.osp` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/http_server_example.test.ospml` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/net_local_address.test.osp` | `osprey_net_local_address` is native code outside the module (not in the portable runtime) |
| `src/osprey/tests/regressions/http/net_local_address.test.ospml` | `osprey_net_local_address` is native code outside the module (not in the portable runtime) |
| `src/osprey/tests/regressions/http/tui_repo_table.test.osp` | HTTP needs sockets: `http_create_server` is outside the module |
| `src/osprey/tests/regressions/http/tui_repo_table.test.ospml` | HTTP needs sockets: `http_create_server` is outside the module |

- [ ] Add a `pipe` model, plus what process spawning needs after it. Acceptance: these programs explore their spawn paths and their rows are deleted. (4 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/regressions/basics/processes/async_process_management.test.osp` | process spawning needs `pipe`, which has no model |
| `src/osprey/tests/regressions/basics/processes/async_process_management.test.ospml` | process spawning needs `pipe`, which has no model |
| `src/osprey/tests/regressions/basics/processes/callback_stdout_demo.test.osp` | process spawning needs `pipe`, which has no model |
| `src/osprey/tests/regressions/basics/processes/callback_stdout_demo.test.ospml` | process spawning needs `pipe`, which has no model |

- [ ] Decide per library whether to model or keep skipping: `PQlibVersion`, `osp_gfx_open`, `ios_host_log`, `osprey_ffi_cell`, `osprey_web_render`, `osprey_python_call`, `term_raw_mode`. Acceptance: each is either modelled and its row deleted, or its row states why it stays. (13 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/examples/db_postgres/pg_smoke.osp` | `PQlibVersion` is native code outside the module (not in the portable runtime) |
| `src/osprey/examples/graphics/character` | `osp_gfx_open` is native code outside the module (not in the portable runtime) |
| `src/osprey/examples/graphics/kali` | `osp_gfx_open` is native code outside the module (not in the portable runtime) |
| `src/osprey/examples/graphics/opal` | `osp_gfx_open` is native code outside the module (not in the portable runtime) |
| `src/osprey/examples/ios/app.osp` | `ios_host_log` is native code outside the module (not in the portable runtime) |
| `src/osprey/examples/projects/modules` | `osprey_ffi_cell` is native code outside the module (not in the portable runtime) |
| `src/osprey/examples/projects/modules/client` | `osprey_web_render` is native code outside the module (not in the portable runtime) |
| `src/osprey/examples/python/interop.osp` | `osprey_python_call` is native code outside the module (not in the portable runtime) |
| `src/osprey/examples/statefulhttp/tui.osp` | `term_raw_mode` is native code outside the module (not in the portable runtime) |
| `src/osprey/tests/regressions/db/database_effect.test.osp` | `osprey_ffi_cell` is native code outside the module (not in the portable runtime) |
| `src/osprey/tests/regressions/db/database_effect.test.ospml` | `osprey_ffi_cell` is native code outside the module (not in the portable runtime) |
| `src/osprey/tests/regressions/db/sqlite_basics.test.osp` | `osprey_ffi_cell` is native code outside the module (not in the portable runtime) |
| `src/osprey/tests/regressions/db/sqlite_basics.test.ospml` | `osprey_ffi_cell` is native code outside the module (not in the portable runtime) |

- [ ] Fix #340 (bug): recursive function awaiting a Fiber parameter fails codegen. Acceptance is on the issue. (1 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/examples/tui/api_browser.osp` | does not compile: recursive `awaitSpinning` awaits a Fiber parameter (#340) |

- [ ] Fix #341 (bug): compare this list to reality. The four `gpt2/model` rows already skip nothing and go with that fix. `staging-type.osp` is a deliberate type error and stays listed. (5 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/examples/llms/gpt2/model/backends.osp` | a module of the gpt2 example, not a program of its own |
| `src/osprey/examples/llms/gpt2/model/cpu.osp` | a module of the gpt2 example, not a program of its own |
| `src/osprey/examples/llms/gpt2/model/gpu.osp` | a module of the gpt2 example, not a program of its own |
| `src/osprey/examples/llms/gpt2/model/model.osp` | a module of the gpt2 example, not a program of its own |
| `src/osprey/examples/handlers/staging-type.osp` | a deliberate type error, kept as a diagnostics example |

## Mobile C ABI targets reject: `MOBILE_UNPORTABLE.txt` (63 rows)

One file covers `ios-sim`, `macos`, `android-arm64` and `android-x64`, because they share one boundary. If a target rejects a different set, the comparison fails and the file must be split rather than widened. Every row is a program the compiler refuses before emitting code, never one that built and then misbehaved. [IOS-TARGET-CAPABILITIES] [IOS-HOST-ABI]

Continuation reasons below end with `(a suspended continuation cannot cross or outlive a synchronous host call)` in the manifest.

- [ ] Support these continuations on the mobile targets. Today the compiler rejects these operations because a suspended continuation cannot cross or outlive a synchronous host call. Shared with [plan 0016](0016-algebraic-effects-and-handlers.md) Phase 4. Acceptance: these programs match the native goldens on `ios-sim`, `macos`, `android-arm64` and `android-x64`, and their rows are deleted. (37 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/effects/multiplicity/multiplicity.test.osp` | a continuation for `Charge.charge` |
| `src/osprey/tests/effects/multiplicity/multiplicity.test.ospml` | a continuation for `Charge.charge` |
| `src/osprey/tests/effects/resume/answer_transform.test.osp` | a continuation for `ControlAsk.value` |
| `src/osprey/tests/effects/resume/answer_transform.test.ospml` | a continuation for `ControlAsk.value` |
| `src/osprey/tests/effects/resume/resume_abort_early_exit.test.osp` | a continuation for `Stop.check` |
| `src/osprey/tests/effects/resume/resume_abort_early_exit.test.ospml` | a continuation for `Stop.check` |
| `src/osprey/tests/effects/resume/resume_error_policies.test.osp` | a continuation for `ResumeFallback.recover` |
| `src/osprey/tests/effects/resume/resume_error_policies.test.ospml` | a continuation for `ResumeFallback.recover` |
| `src/osprey/tests/effects/resume/resume_lifo_audit.test.osp` | a continuation for `Audit.step` |
| `src/osprey/tests/effects/resume/resume_lifo_audit.test.ospml` | a continuation for `Audit.step` |
| `src/osprey/tests/effects/resume/resume_outer_handler_bridge.test.osp` | a continuation for `Prompt.ask` |
| `src/osprey/tests/effects/resume/resume_outer_handler_bridge.test.ospml` | a continuation for `Prompt.ask` |
| `src/osprey/tests/effects/resume/resume_unit_markers.test.osp` | a continuation for `Trace.mark` |
| `src/osprey/tests/effects/resume/resume_unit_markers.test.ospml` | a continuation for `Trace.mark` |
| `src/osprey/tests/effects/resume/resume_value_rewrite.test.osp` | a continuation for `Supply.next` |
| `src/osprey/tests/effects/resume/resume_value_rewrite.test.ospml` | a continuation for `Supply.next` |
| `src/osprey/tests/effects/scope/finalizers_masks_instances.test.osp` | a continuation for `Stop.halt` |
| `src/osprey/tests/effects/scope/finalizers_masks_instances.test.ospml` | a continuation for `Stop.halt` |
| `src/osprey/tests/modules/ml_layout_depth.test.osp` | a continuation for `depth::Trunk::Branch::Bud::Sap.draw` |
| `src/osprey/tests/modules/ml_layout_depth.test.ospml` | a continuation for `depth::Trunk::Branch::Bud::Sap.draw` |
| `src/osprey/tests/modules/module_effects_rows.test.osp` | a continuation for `pipeline::Source::Feed.next` |
| `src/osprey/tests/modules/module_effects_rows.test.ospml` | a continuation for `pipeline::Source::Feed.next` |
| `src/osprey/tests/modules/signature_ascription.test.osp` | a continuation for `store::Clocks::Clock.now` |
| `src/osprey/tests/modules/signature_ascription.test.ospml` | a continuation for `store::Clocks::Clock.now` |
| `src/osprey/tests/regressions/effects/abort_vs_resume.test.osp` | a continuation for `Step.run` |
| `src/osprey/tests/regressions/effects/abort_vs_resume.test.ospml` | a continuation for `Step.run` |
| `src/osprey/tests/regressions/effects/fiber_effects.test.osp` | a continuation for `Gift.get` |
| `src/osprey/tests/regressions/effects/fiber_effects.test.ospml` | a continuation for `Gift.get` |
| `src/osprey/tests/regressions/effects/generic_handler_value.test.osp` | a continuation for `Fault.halt` |
| `src/osprey/tests/regressions/effects/generic_handler_value.test.ospml` | a continuation for `Fault.halt` |
| `src/osprey/tests/regressions/effects/handler_scoping.test.osp` | a continuation for `Ask.ask` |
| `src/osprey/tests/regressions/effects/handler_scoping.test.ospml` | a continuation for `Ask.ask` |
| `src/osprey/tests/regressions/effects/retry_until_valid.test.osp` | a continuation for `Prompt.ask` |
| `src/osprey/tests/regressions/effects/retry_until_valid.test.ospml` | a continuation for `Prompt.ask` |
| `src/osprey/tests/regressions/effects/staged_shared.test.osp` | a continuation for `Audit.note` |
| `src/osprey/tests/regressions/effects/staged_shared.test.ospml` | a continuation for `Audit.note` |
| `src/osprey/tests/regressions/fiber/cpu_profiling_demo.test.osp` | a continuation for `Tuning.baseline` |

- [ ] Give HTTP builtins a mobile implementation through the host, shared with [plan 0037](0037-osprey-native.md) ("Every host completes `Http`"). Acceptance: these programs match the native goldens on all four targets and their rows are deleted. (14 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/regressions/effects/http_state_levels.test.osp` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/effects/http_state_levels.test.ospml` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/http/chunked_hostile_server.test.osp` | HTTP builtin `httpResponseFree` |
| `src/osprey/tests/regressions/http/chunked_hostile_server.test.ospml` | HTTP builtin `httpResponseFree` |
| `src/osprey/tests/regressions/http/http_client_example.test.osp` | HTTP builtin `httpCreateServer` |
| `src/osprey/tests/regressions/http/http_client_example.test.ospml` | HTTP builtin `httpCreateServer` |
| `src/osprey/tests/regressions/http/http_create_client.test.osp` | HTTP builtin `httpCreateClient` |
| `src/osprey/tests/regressions/http/http_create_client.test.ospml` | HTTP builtin `httpCreateClient` |
| `src/osprey/tests/regressions/http/http_response_handle.test.osp` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/http/http_response_handle.test.ospml` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/http/http_server_example.test.osp` | HTTP builtin `httpCreateServer` |
| `src/osprey/tests/regressions/http/http_server_example.test.ospml` | HTTP builtin `httpCreateServer` |
| `src/osprey/tests/regressions/http/tui_repo_table.test.osp` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/http/tui_repo_table.test.ospml` | HTTP builtin `httpResponseBody` |

- [ ] Give the WebSocket builtins the same treatment as HTTP. Acceptance: these programs match the native goldens and their rows are deleted. (2 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/regressions/http/websocket_connections.test.osp` | WebSocket builtin `websocketServerBroadcast` |
| `src/osprey/tests/regressions/http/websocket_connections.test.ospml` | WebSocket builtin `websocketServerBroadcast` |

- [ ] Decide per target whether process spawning exists: no iOS implementation exists today, and the manifest cannot express a per-target answer. Splitting the file is required if the targets diverge, because the header forbids widening it. Acceptance: implemented with rows deleted, or the file is split and each target's rows are justified. (4 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/regressions/basics/processes/async_process_management.test.osp` | process spawning builtin `spawnProcess` |
| `src/osprey/tests/regressions/basics/processes/async_process_management.test.ospml` | process spawning builtin `spawnProcess` |
| `src/osprey/tests/regressions/basics/processes/callback_stdout_demo.test.osp` | process spawning builtin `spawnProcess` |
| `src/osprey/tests/regressions/basics/processes/callback_stdout_demo.test.ospml` | process spawning builtin `spawnProcess` |

- [ ] Make `// @link: sqlite3` and `// @link: ssl` work in a mobile archive. Acceptance: these programs match the native goldens and their rows are deleted. (6 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/regressions/db/database_effect.test.osp` | link directive `// @link: sqlite3` |
| `src/osprey/tests/regressions/db/database_effect.test.ospml` | link directive `// @link: sqlite3` |
| `src/osprey/tests/regressions/db/sqlite_basics.test.osp` | link directive `// @link: sqlite3` |
| `src/osprey/tests/regressions/db/sqlite_basics.test.ospml` | link directive `// @link: sqlite3` |
| `src/osprey/tests/regressions/http/net_local_address.test.osp` | link directive `// @link: ssl` |
| `src/osprey/tests/regressions/http/net_local_address.test.ospml` | link directive `// @link: ssl` |

## wasm32 rejects: `WASM_UNPORTABLE.txt` (84 rows)

File operations, stdin, random numbers and substituting effects are supported and are not listed. Each row names a feature with no wasm implementation. [WASM-TARGET-CAPABILITIES] Tracking issue: #30.

Continuation reasons below end with `(wasm32 acquires one-shot continuations when the stack-switching proposal lands)` in the manifest.

- [ ] Implement one-shot continuations on wasm32 when the stack-switching proposal lands. Tracked by [plan 0016](0016-algebraic-effects-and-handlers.md) Phase 4. Acceptance: these programs match the native goldens and their rows are deleted. (34 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/effects/multiplicity/multiplicity.test.osp` | a continuation for `Charge.charge` |
| `src/osprey/tests/effects/multiplicity/multiplicity.test.ospml` | a continuation for `Charge.charge` |
| `src/osprey/tests/effects/resume/answer_transform.test.osp` | a continuation for `ControlAsk.value` |
| `src/osprey/tests/effects/resume/answer_transform.test.ospml` | a continuation for `ControlAsk.value` |
| `src/osprey/tests/effects/resume/resume_abort_early_exit.test.osp` | a continuation for `Stop.check` |
| `src/osprey/tests/effects/resume/resume_abort_early_exit.test.ospml` | a continuation for `Stop.check` |
| `src/osprey/tests/effects/resume/resume_error_policies.test.osp` | a continuation for `ResumeFallback.recover` |
| `src/osprey/tests/effects/resume/resume_error_policies.test.ospml` | a continuation for `ResumeFallback.recover` |
| `src/osprey/tests/effects/resume/resume_lifo_audit.test.osp` | a continuation for `Audit.step` |
| `src/osprey/tests/effects/resume/resume_lifo_audit.test.ospml` | a continuation for `Audit.step` |
| `src/osprey/tests/effects/resume/resume_outer_handler_bridge.test.osp` | a continuation for `Prompt.ask` |
| `src/osprey/tests/effects/resume/resume_outer_handler_bridge.test.ospml` | a continuation for `Prompt.ask` |
| `src/osprey/tests/effects/resume/resume_unit_markers.test.osp` | a continuation for `Trace.mark` |
| `src/osprey/tests/effects/resume/resume_unit_markers.test.ospml` | a continuation for `Trace.mark` |
| `src/osprey/tests/effects/resume/resume_value_rewrite.test.osp` | a continuation for `Supply.next` |
| `src/osprey/tests/effects/resume/resume_value_rewrite.test.ospml` | a continuation for `Supply.next` |
| `src/osprey/tests/effects/scope/finalizers_masks_instances.test.osp` | a continuation for `Stop.halt` |
| `src/osprey/tests/effects/scope/finalizers_masks_instances.test.ospml` | a continuation for `Stop.halt` |
| `src/osprey/tests/modules/ml_layout_depth.test.osp` | a continuation for `depth::Trunk::Branch::Bud::Sap.draw` |
| `src/osprey/tests/modules/ml_layout_depth.test.ospml` | a continuation for `depth::Trunk::Branch::Bud::Sap.draw` |
| `src/osprey/tests/modules/module_effects_rows.test.osp` | a continuation for `pipeline::Source::Feed.next` |
| `src/osprey/tests/modules/module_effects_rows.test.ospml` | a continuation for `pipeline::Source::Feed.next` |
| `src/osprey/tests/modules/signature_ascription.test.osp` | a continuation for `store::Clocks::Clock.now` |
| `src/osprey/tests/modules/signature_ascription.test.ospml` | a continuation for `store::Clocks::Clock.now` |
| `src/osprey/tests/regressions/effects/abort_vs_resume.test.osp` | a continuation for `Step.run` |
| `src/osprey/tests/regressions/effects/abort_vs_resume.test.ospml` | a continuation for `Step.run` |
| `src/osprey/tests/regressions/effects/generic_handler_value.test.osp` | a continuation for `Fault.halt` |
| `src/osprey/tests/regressions/effects/generic_handler_value.test.ospml` | a continuation for `Fault.halt` |
| `src/osprey/tests/regressions/effects/handler_scoping.test.osp` | a continuation for `Ask.ask` |
| `src/osprey/tests/regressions/effects/handler_scoping.test.ospml` | a continuation for `Ask.ask` |
| `src/osprey/tests/regressions/effects/retry_until_valid.test.osp` | a continuation for `Prompt.ask` |
| `src/osprey/tests/regressions/effects/retry_until_valid.test.ospml` | a continuation for `Prompt.ask` |
| `src/osprey/tests/regressions/effects/staged_shared.test.osp` | a continuation for `Audit.note` |
| `src/osprey/tests/regressions/effects/staged_shared.test.ospml` | a continuation for `Audit.note` |

- [ ] Give wasm32 fiber concurrency, `Channel` and `sleep`, which need threads today. Depends on the wasm story [plan 0026](0026-structured-concurrency.md) leaves open. Acceptance: these programs match the native goldens and their rows are deleted. (22 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/core/collections/nested_generic_collections_fibers.test.osp` | fiber concurrency |
| `src/osprey/tests/core/collections/nested_generic_collections_fibers.test.ospml` | fiber concurrency |
| `src/osprey/tests/modules/module_composition.test.osp` | fiber concurrency |
| `src/osprey/tests/modules/module_composition.test.ospml` | fiber concurrency |
| `src/osprey/tests/regressions/basics/osprey_mega_showcase.test.osp` | fiber concurrency |
| `src/osprey/tests/regressions/basics/osprey_mega_showcase.test.ospml` | fiber concurrency |
| `src/osprey/tests/regressions/basics/types/channel_element_type_in_generic_field.test.osp` | fiber concurrency builtin `Channel` |
| `src/osprey/tests/regressions/basics/types/sized_integers_fibers.test.osp` | fiber concurrency |
| `src/osprey/tests/regressions/basics/types/sized_integers_fibers.test.ospml` | fiber concurrency |
| `src/osprey/tests/regressions/basics/website/website_examples.test.osp` | fiber concurrency |
| `src/osprey/tests/regressions/basics/website/website_examples.test.ospml` | fiber concurrency |
| `src/osprey/tests/regressions/effects/algebraic_effects_comprehensive.test.osp` | fiber concurrency |
| `src/osprey/tests/regressions/effects/algebraic_effects_comprehensive.test.ospml` | fiber concurrency |
| `src/osprey/tests/regressions/effects/fiber_effects.test.osp` | fiber concurrency |
| `src/osprey/tests/regressions/effects/fiber_effects.test.ospml` | fiber concurrency |
| `src/osprey/tests/regressions/fiber/channel_select.test.osp` | fiber concurrency builtin `Channel` |
| `src/osprey/tests/regressions/fiber/channel_select.test.ospml` | fiber concurrency builtin `Channel` |
| `src/osprey/tests/regressions/fiber/cpu_profiling_demo.test.osp` | fiber concurrency builtin `sleep` |
| `src/osprey/tests/regressions/fiber/fiber_showcase.test.osp` | fiber concurrency |
| `src/osprey/tests/regressions/fiber/fiber_showcase.test.ospml` | fiber concurrency |
| `src/osprey/tests/regressions/fiber/many_handles.test.osp` | fiber concurrency |
| `src/osprey/tests/regressions/fiber/many_handles.test.ospml` | fiber concurrency |

- [ ] Decide whether `spawnProcess` exists on wasm32, since it needs fork/exec. Acceptance: implemented with rows deleted, or the rows state that it is permanent. (4 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/regressions/basics/processes/async_process_management.test.osp` | process spawning builtin `spawnProcess` |
| `src/osprey/tests/regressions/basics/processes/async_process_management.test.ospml` | process spawning builtin `spawnProcess` |
| `src/osprey/tests/regressions/basics/processes/callback_stdout_demo.test.osp` | process spawning builtin `spawnProcess` |
| `src/osprey/tests/regressions/basics/processes/callback_stdout_demo.test.ospml` | process spawning builtin `spawnProcess` |

- [ ] Give wasm32 the HTTP builtins, which need sockets today. Acceptance: these programs match the native goldens and their rows are deleted. (14 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/regressions/effects/http_state_levels.test.osp` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/effects/http_state_levels.test.ospml` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/http/chunked_hostile_server.test.osp` | HTTP builtin `httpResponseFree` |
| `src/osprey/tests/regressions/http/chunked_hostile_server.test.ospml` | HTTP builtin `httpResponseFree` |
| `src/osprey/tests/regressions/http/http_client_example.test.osp` | HTTP builtin `httpCreateServer` |
| `src/osprey/tests/regressions/http/http_client_example.test.ospml` | HTTP builtin `httpCreateServer` |
| `src/osprey/tests/regressions/http/http_create_client.test.osp` | HTTP builtin `httpCreateClient` |
| `src/osprey/tests/regressions/http/http_create_client.test.ospml` | HTTP builtin `httpCreateClient` |
| `src/osprey/tests/regressions/http/http_response_handle.test.osp` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/http/http_response_handle.test.ospml` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/http/http_server_example.test.osp` | HTTP builtin `httpCreateServer` |
| `src/osprey/tests/regressions/http/http_server_example.test.ospml` | HTTP builtin `httpCreateServer` |
| `src/osprey/tests/regressions/http/tui_repo_table.test.osp` | HTTP builtin `httpResponseBody` |
| `src/osprey/tests/regressions/http/tui_repo_table.test.ospml` | HTTP builtin `httpResponseBody` |

- [ ] Resolve foreign C imports on wasm32: `sqlite3_open`, `fiber_set_deterministic_mode`, `osprey_net_local_address`, `websocket_stop_server`. The two documented browser host imports stay supported. Acceptance: each import is compiled to wasm or supplied by the host, and its rows are deleted. (10 rows)

| Program | Reason |
| --- | --- |
| `src/osprey/tests/regressions/db/database_effect.test.osp` | foreign C import `sqlite3_open` |
| `src/osprey/tests/regressions/db/database_effect.test.ospml` | foreign C import `sqlite3_open` |
| `src/osprey/tests/regressions/db/sqlite_basics.test.osp` | foreign C import `sqlite3_open` |
| `src/osprey/tests/regressions/db/sqlite_basics.test.ospml` | foreign C import `sqlite3_open` |
| `src/osprey/tests/regressions/fiber/fiber_determinism.test.osp` | foreign C import `fiber_set_deterministic_mode` |
| `src/osprey/tests/regressions/fiber/fiber_exact_replica.test.osp` | foreign C import `fiber_set_deterministic_mode` |
| `src/osprey/tests/regressions/http/net_local_address.test.osp` | foreign C import `osprey_net_local_address` |
| `src/osprey/tests/regressions/http/net_local_address.test.ospml` | foreign C import `osprey_net_local_address` |
| `src/osprey/tests/regressions/http/websocket_connections.test.osp` | foreign C import `websocket_stop_server` |
| `src/osprey/tests/regressions/http/websocket_connections.test.ospml` | foreign C import `websocket_stop_server` |
