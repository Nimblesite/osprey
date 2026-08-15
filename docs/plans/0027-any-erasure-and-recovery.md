# `any` erasure: rows, structural recovery and ownership

**Status:** analysis complete, design decided (§7), no fix started. The decision
adds rows, anonymous records, tuples and structural patterns to the language
surface — [TYPE-ROW], [TYPE-RECORD-ANON], [TYPE-TUPLE], [PATTERN-STRUCTURAL],
[PATTERN-TUPLE], [FLAVOR-ML-TUPLE] — because narrowing an `any` is the same
mechanism as matching a row.
**Invariant under audit:** *a program the checker accepts either behaves as
documented or is rejected with a truthful error — it never prints a wrong answer
and never reads freed memory.*
**Audited against:** `crates/osprey-codegen/src/{lower,arc,cast,effects}.rs`,
`crates/osprey-types/src/{expr,unify,ty}.rs`, spec 0004 `[TYPE-ANY]`, spec 0018
`[MEM-BACKENDS]`.

Every observation below was produced by compiling and running a probe against
`target/release/osprey`, on this branch **and** on a clean `origin/main`
worktree at `a57673e2`. All three findings reproduce identically on both, in
both flavors, so none of them is a branch regression. Observed output is quoted
verbatim.

Normative behaviour of `any` stays in spec 0004 `[TYPE-ANY]`. This plan holds
only what is *broken* about it, and is deleted when the checklist is done.

---

## 1. Verdict

`any` is erased to a machine word at lowering, and the word keeps no evidence of
what it was. Three consequences follow, and they are not variations of one bug —
they fail in different places, on different backends, with different symptoms.

| # | Finding | Backends | Symptom | Issue |
|---|---------|----------|---------|-------|
| A | A heap value returned as `any` is released by the producing frame | `arc` only | Dangling read; two calls over-release | [#208](https://github.com/Nimblesite/osprey/issues/208) |
| B | A `let` annotation does not drive the recovery coercion a return type does | all | Prints the address as a decimal integer | [#209](https://github.com/Nimblesite/osprey/issues/209) |
| C | Recovering a pointer from a word that never was one is unchecked | all | SIGSEGV, no diagnostic | — |

The common cause is representational: `LType::I64` is simultaneously every
`int`, every erased `any`, and every *borrowed* `any` parameter. Codegen cannot
distinguish them, so neither an ownership rule nor a recovery rule can key off
anything real. **Every fix below is blocked on removing that conflation**; the
findings are listed separately because they need different repairs once it is.

---

## 2. Finding A — an erasing return frees its own referent (`--memory=arc`)

```osprey
fn erased() -> any = "a" + "b"
fn read() -> string = erased()
print("e=${read()}")
```

```
$ OSPREY_ARC_DEBUG=1 osprey a.osp --run --quiet --memory=arc
[osp-arc] exit: 0 live objects, 0 KiB (+0 immortal)
e=
```

`--memory=default` and `--memory=gc` print `e=ab`.

Two recoveries in one expression over-release rather than merely dangle:

```osprey
fn erased() -> any = "abcdefghijklmnopqrstuvwxyz" + "0123456789"
fn read() -> string = erased()
print("len=${length(read())} val=${read()}")
```

```
$ OSPREY_ARC_DEBUG=1 osprey b.osp --run --quiet --memory=arc
[osp-arc] exit: 18446744073709551615 live objects, 18014398509481983 KiB (+0 immortal)
len=0 val=
```

`18446744073709551615` is `-1` unsigned: the ledger released an object it no
longer held.

**Cause.** `coerce_return` erases the returned pointer to the declared `i64`
return type (`crates/osprey-codegen/src/lower.rs`). ARC matches owners by SSA
operand, so the `ptrtoint` result is a different register than the recorded
owner and `take_owner_anywhere` misses. The compensating `retain_val` in
`epilogue` is *also* a no-op, because `managed()` requires `LType::Str |
LType::Ptr` and the erased value is now `LType::I64`. Neither the move-out nor
the retain fires, so the frame releases the string while the caller holds the
word.

## 3. Finding B — recovery is positional

The same erased value recovers correctly through a return type and silently
miscompiles through a `let` annotation, in one program:

```osprey
fn erased() -> any = "a" + "b"
fn viaReturn() -> string = erased()

let viaLet: string = erased()

print("viaReturn=${viaReturn()} viaLet=${viaLet}")
```

```
$ osprey c.osp --check
c.osp: ok (4 statements)
$ osprey c.osp --run --quiet --memory=default
viaReturn=ab viaLet=4385840928
$ osprey c.osp --run --quiet --memory=gc
viaReturn=ab viaLet=4351876928
```

`viaLet` is the heap address rendered as an integer, and differs on every run.
The ML twin behaves identically.

**Cause.** `coerce_return` coerces the body to the *declared* return `LType`,
and that is what emits the `inttoptr`. `Stmt::Let` has no equivalent: it lowers
the initializer and binds it as-is, never consulting the declared type. The
comment there states the intent — *"Bindings preserve their inferred
representation"* — which is right for every type whose lowered form already
matches its annotation, and wrong for the one type where it does not.

## 4. Finding C — un-erasure is an unchecked assertion

`any` unifies with everything and carries no tag, so nothing rejects recovering
a pointer from a word that was never one:

```osprey
fn intish() -> any = 7
fn useIt() -> string = intish()
print("x=${length(useIt())}")
```

```
$ osprey d.osp --run --quiet --memory=default; echo "rc=$?"
rc=139
```

`139` is SIGSEGV. This is the concrete cost of "code that consumes its
representation must already know what was passed": today every un-erasure is an
unchecked assertion, and the checker offers no way to make it a checked one.

---

## 5. What already works, and must keep working

*Forwarding* an erased value is safe — the word is borrowed and the frame that
built the value still owns it. `tests/regressions/basics/types/
any_type_comprehensive.test.{osp,ospml}` pins it under all three backends with
the live-object oracle armed, and
`recovering_a_pointer_from_an_erased_word_takes_no_ownership`
(`crates/osprey-codegen/src/lib.rs`) fails if an owner is ever entered in the
ledger at that cast.

```osprey
fn forward(x: any) -> any = x
fn forwarded() -> string = forward("dyn" + "amic")
```

## 6. The repair that is wrong — do not retry it

Transferring the reference when erasing and taking ownership when recovering was
written, shipped briefly on this branch, and reverted. It balances **only** when
the word came from a `pointer -> any` erasure:

- `fn identity(x: any) -> any = x` gains an owner it never received. The
  epilogue moves that fictitious owner out, releases the real one, and returns a
  dangling pointer — observed as `v=` where `origin/main` prints `v=ab`.
- `fn intish() -> any = 7` registers `7` as a pointer and later frees it.

So the rule is not merely unbalanced, it is memory-unsafe in the *other*
direction, and it converts finding C from a crash into a heap corruption. Any
proposal that keys ownership off the machine word has this same flaw.

## 7. The decision — `any` carries its row

**An erased value carries a runtime row descriptor, and the only way to read
through an `any` is a structural `match`.** Recovery by declared type — the
mechanism behind both A and B — is deleted, not repaired: `coerce_return`'s
`inttoptr` and any `Stmt::Let` equivalent are the unchecked assertion, and an
annotation must never resurrect a pointer. This is normative in
[TYPE-ANY](../specs/0004-TypeSystem.md#the-any-type--type-any) and
[PATTERN-STRUCTURAL](../specs/0007-PatternMatching.md#structural-patterns--pattern-structural).

That fixes all three findings at once. B disappears because no annotation
coerces. C disappears because an arm naming a field a scalar row lacks does not
select. A becomes decidable because the descriptor says whether the word is a
pointer, so ownership stops keying off the machine word — the exact flaw that
sank the reverted repair in §6.

Two facts constrain the descriptor, both verified in the tree:

- **The ARC `meta` word cannot serve.** It encodes which words are managed
  pointers, not which fields exist, and `compiler/runtime/memory_hooks.h` states
  that default and GC *ignore* it. Narrowing must work on all three backends and
  on wasm32, so the descriptor belongs to the value representation, not the
  allocator header.
- **The row encoding already exists.** `positional_field_name`
  (`crates/osprey-ast/src/lib.rs`) names positional payload slots `"0"`, `"1"`,
  …, which is exactly the tuple encoding
  ([TYPE-TUPLE](../specs/0004-TypeSystem.md#tuples--type-tuple)). Records,
  tuples and positional variant payloads are one row concept, so one descriptor
  covers them.

---

## TODO

### Phase 1 — the row descriptor

- [ ] One runtime row descriptor per heap value, in the value representation and
      identical on default, GC, ARC and wasm32. Records, tuples and positional
      variant payloads share it; scalars carry the empty row.
- [ ] Give an erased `any` its own `LType` distinct from `I64` so
      `coerce_return`, `Stmt::Let` and the effect mailbox can see the erasure.
- [ ] `slot_is_managed` (`crates/osprey-codegen/src/effect_mailbox.rs`)
      classifies an erased operand from the descriptor, not its ABI.
- [ ] `cross_flavor_ir_equiv` stays green.

### Phase 2 — delete recovery-by-annotation (#209, finding C)

- [ ] Remove the `inttoptr` in `coerce_return` that recovers a pointer from a
      declared return type; add no `Stmt::Let` equivalent.
- [ ] `examples/failscompilation/`: reading a field of an `any` without a
      structural match, and `fn intish() -> any = 7` recovered as a `string`.
- [ ] Corpus case: the `viaReturn`/`viaLet` program above, now rejected rather
      than printing an address.

### Phase 3 — structural narrowing and ownership (#208)

- [ ] `{ x }` / `{ x, .. }` / `(a, b)` patterns lower against the descriptor,
      both flavors ([PATTERN-STRUCTURAL], [PATTERN-TUPLE]).
- [ ] Ownership keys off the descriptor, never the machine word (§6).
- [ ] Corpus cases with `OSPREY_ARC_DEBUG=1`: erased value narrowed once and
      twice, through an effect operand, and discarded while still erased. Each
      asserts zero live objects.

### Phase 5 — verification

- [ ] `make ci` green; differential harness under all three backends **and**
      `OSPREY_TARGET=wasm32`.
- [ ] Re-run every probe in this plan and confirm the quoted output changed.
- [ ] Close #208 and #209 with the corpus case that pins each.
- [ ] Delete this plan and its README row.
