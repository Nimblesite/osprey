# Review: `fixes` (d1c001243) vs `main` (6ad8700e3)

Scope: 242 files. Reviewed in AGENTS.md priority order. Every finding below was reproduced or verified by reading the code at d1c001243; reproductions used the existing `target/release/osprey` (built after the branch commit) on scratch files outside the repo.

## Findings

### 1. MEDIUM — Python `if`-join lowering emits duplicate names when the first branch assigns a joined name twice (false rejection)

- **File:** `src/toolchain/crates/osprey-syntax/src/python/joins.rs:12-24` (`joined_names`) and `:47-53` (`count_joined_writes`)
- **What is wrong:** `suite_binds` returns one node per assignment statement, so a first branch that writes the same name twice yields `first = [a, a]`. `joined_names` filters `first` without de-duplicating, so `names = [a, a]`. Then:
  - `count_joined_writes` subtracts `extra` once per duplicate: `{a: 3}` → `2` → `1`, so `a` is planned **immutable**, and the branch's second write is refused.
  - `joined_if` would build `Expr::Object[{a: a}, {a: a}]` (a record with duplicate fields) if it ever got past lowering.
- **Failure scenario (verified):** in a context where a double assignment is legal (inside a handler arm under an outer `Arith` handler):

  ```python
  # osprey: flavor=python
  def bump(x):
      return x + 1
  @handler(Arith)
  class outer:
      def overflow(_op, lhs, rhs, _width):
          return lhs
  @handler(Arith)
  class wrap:
      def overflow(_op, lhs, rhs, _width):
          if lhs > rhs:
              total = lhs
              total = total + 1
          else:
              total = rhs
          return total
  print(f"{outer(lambda: wrap(lambda: bump(9223372036854775807)))}")
  ```
  → `exit=1`, `handler_double.py:16:12: binding 'total' cannot be reassigned: it is immutable or declared Final`.
  The same body without the `if` (`total = lhs; total = total + 1`) compiles and prints `9223372036854775807`. Moving the double write to the `elif` branch instead of the first branch also compiles (count reaches 2 → mutable), so the result depends on which branch holds the write — an inconsistency in itself.
- **Contradicts** spec 0041 [FLAVOR-PYTHON-IF-JOIN]: "A name that every branch of an `if`/`elif`/`else` binds … is bound after it".
- **Fix direction:** de-duplicate `first` keeping first-occurrence order (a `BTreeSet`/`Vec::dedup` keyed on text) before filtering, so `count_joined_writes` and `joined_if` see each name once. Add a `boolean_case` row with a name written twice in the first branch.

### 2. LOW — Spec 0011 says a wasm32 `select` with a timeout sleeps to its deadline; the code (and the same spec, 90 lines later) say it does not

- **File:** `docs/specs/language/0011-LightweightFibersAndConcurrency.md:170` vs `:337-339`; code `src/toolchain/compiler/runtime/fiber_channels.h:298-305`
- **What is wrong:** `channel_select` only calls `select_until_ready` in deterministic mode when `timeout_ms < 0`. With `timeout_ms > 0` and nothing queued, `select_deterministic` returns `count` and the timeout arm is taken at once. Line 337-339 states exactly that ("Once none is left, `_` or `timeout` runs without waiting for the clock"), but the new wasm32 section says "A `select` with a timeout sleeps until its deadline."
- **Verified:** compiled `select { value = recv(first) => …; timeout(1500) => 0 }` for wasm32 and ran it under wasmtime: `chosen=0`, elapsed 0 s.
- Minor, same table (lines 70-72): the fiber-lifetime row says holds are dropped "at the next `spawn` or the end of `main`", but `channel_create` also calls `held_reclaim()` (the channel row two tables later says so).

### 3. LOW — Spec 0018 contradicts itself on ownership acceptance

- **File:** `docs/specs/compiler/0018-MemoryManagement.md:763` ("accepts 140 of the 342 programs") vs `:765` ("accepts 82 of the 180 programs in `src/osprey/tests/` (2026-11-24)"). The second sentence was left stale.

### 4. LOW — Stale comment: fiber/channel handles are no longer ids

- **File:** `src/toolchain/crates/osprey-codegen/src/types.rs:86-87` — "A handle itself has no owner (`owner_name` answers `None` for `Fiber` and `Channel`: they are runtime ids, not aggregates)". The same file (`:56-63`) now lowers both to `LType::Ptr` managed handles, and `fiber.rs`/`arc.rs` treat them as owned pointers. (Same wording also survives in `builder/signatures.rs:21-22`: "a handle's own `osp_ty` is empty (an id is a machine word owning nothing)".)

### 5. LOW — Redundant type annotations added to a touched `.osp`/`.ospml`

- **File:** `src/osprey/tests/regressions/basics/types/let_alias_closure_instantiation.test.osp:25`, `.test.ospml:36`
- **What is wrong:** the conversion from `rows<A>` to a monomorphic `rows` added `picked: Action`, `extra: int` and `-> Node<Action>`. Only `entries: List<Row<Action>>` is uninferable (`length(entries)` leaves the element free). Verified: `osprey <variant> --check` passes with `fn rows(entries: List<Row<Action>>, picked, extra) = match picked { … }` and the same output. AGENTS.md: "redundant type annotations are defects … Applies to every `.osp` you touch".

## Verified clean (what was checked, no issue found)

**C runtime (`src/toolchain/compiler/runtime`)**
- `handle_lifetime.h`: holder count (`held_hold`/`held_release`), Treiber push (`held_handle_finalize`) and whole-list drain (`held_reclaim`) are correct under C11 (release sequence headed by the first release CAS covers later RMW pushes; `dead_next` is read before the release that may free). Exactly one `HeldHandle` exists per `Held` (`fiber_spawn_env_owned`, `channel_create` are the only writers of `handle->held`), and each backend runs a block's finalizer once (ARC at rc 0, ownership at the explicit free, GC in `gc_finalize_dead` immediately before the sweep frees the cell), so a `Held` is pushed at most once — no double-decrement/self-loop.
- `fiber_runtime.c`: runner hold released only after the body ran on every path (`execute_fiber_directly`, `fiber_thread_func`); queued fibers keep the runner hold until claimed, so `fiber_free`'s `handler_snapshot != NULL` branch (correctly `osp_mem_scanned_free` — `effects_runtime.c` maps `malloc`/`free` to scanned blocks via `effects_scanned.h`) is unreachable from compiled programs. Result ownership is balanced: thunk +1 held by the runtime, `osp_retain` per `await`, released in `fiber_free`. A joiner always holds its own hold, so the fiber cannot be freed under `fiber_join`.
- `fiber_channels.h`: every operation takes a hold via `held_of_handle` and drops it after unlocking the channel mutex; `channel_free` runs with no channel mutex held; `channel_create` → `held_handle_new` → `osp_alloc_refused` is `_Noreturn`, so the following `handle->held` store cannot see NULL. `select` releases non-chosen managed sends; hoisted `i8**` slots are stack roots under GC.
- Deadlock detector (`memory_gc_waits.h`, `gc_collect_locked`): deferred stacks/records are scanned once reached or at settle, so nothing a waiting fiber holds is swept; the fixpoint is monotone and conservative (false negatives only, as documented). `g_waits` is heap-locked; waits register before parking, so a collector never sees a half-registered wait.
- `memory_ownership.c` (`OSP_OWN_HELD` state, `osp_ownership_share`, `own_keep`), `memory_finalizers.h` subtype dispatch, `memory_arc.c`/`memory_gc.c` finalizer plumbing, `stack_guard.h` (constructor now in the backend, no `fflush` in the signal path), `http_server_*` hold/release pairing (unregister serialises stop; `destroy_server` releases the loop's hold once), KLEE pthread condvar mocks — no defects found.
- `memory_parked_wait.h` single-thread path: the bounded branch's `(tv_sec diff) * 1e9` can overflow, but it is unreachable on wasm32/KLEE because `select_until_ready` is only entered with `timeout_ms < 0` (NULL deadline); not reported as a bug.

**Rust crates**
- No new `unwrap()`/`panic!`/`#[allow]` in the diff; `reject_less_general` (osprey-types) only rejects binders that are no longer distinct unbound vars; `constructor_arity`, `bare_handle`/`reject_bare_fields`, osprey-project cache (`dependency_digests`, `fresh`), `parse_text_in` discipline routing (unknown disciplines still error in `discipline::apply`), osprey-cli `wasm.rs`/`target_capabilities.rs`, `wasm-smoke-support.mjs` worker re-exec — no defects found.
- `arc/unwind.rs` debug-location threading, `second_owner_of`/`names_handle`, `count_call`, `select_call` hoisting — consistent; ownership sharing of channels and fiber handles verified end-to-end (`--memory=ownership` runs `relay(box, n)` sharing a channel with spawned fibers, also via a record field `holder.ch`).

**Tests / CI**
- No weakened gates: ownership corpus now runs every program (listed must pass, rest must be refused by name), floors raised (137→140 accepted, wasm goldens 216→278), `HUNT_UNPORTABLE`/`WASM_UNPORTABLE` rows removed because fibers are now modelled/portable, `fiber_showcase` honestly relabelled as the engine's stack limit. Proof mutations: `active-finalizer` removed because `osp_mem_set_finalizer` is no longer inert under ownership; new `FiberLifetime`/`FiberDeadlock` mutants added; Lean files contain no `sorry`/`native_decide`.
- Removed tests (`test_unassigned_fiber_handles`, `test_spawn_refused_by_a_full_handle_table_frees_the_handler_copy`, `test_cleanup_drains_unreceived_values`) test concepts that no longer exist (id table, channel cleanup walk) and are replaced by lifetime-equivalent tests with assertions at least as strong.

**Osprey sources**
- New `.ospml` under `examples/disciplines/{dependent,lean,*_roundtrip}` and `hunt/{fibers,fiber_deadlock}.osp`: no panicking builtins, no fake zero values for errors, no redundant annotations. `channel_recv`'s `-1` for a NULL/empty handle is pre-existing and unreachable from compiled code (`channel_create_checked` stops the run).
