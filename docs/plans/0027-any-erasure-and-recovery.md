# `any` erasure: rows, structural recovery and ownership

**Status:** design decided (§7); **recovery by annotation is deleted** — findings
B and C are closed, A and D are open and both need the runtime descriptor (§8).
The decision adds rows, anonymous records, tuples and structural patterns to the
language surface — [TYPE-ROW], [TYPE-RECORD-ANON], [TYPE-TUPLE],
[PATTERN-STRUCTURAL], [PATTERN-TUPLE], [FLAVOR-ML-TUPLE] — because narrowing an
`any` is the same mechanism as matching a row.
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
what it was. Four consequences follow, and they are not variations of one bug —
they fail in different places, on different backends, with different symptoms.

| # | Finding | Backends | Symptom | Issue | State |
|---|---------|----------|---------|-------|-------|
| A | A heap value returned as `any` is released by the producing frame | `arc` only | Dangling read; two calls over-release | [#208](https://github.com/Nimblesite/osprey/issues/208) | open — needs §8 |
| B | A `let` annotation does not drive the recovery coercion a return type does | all | Prints the address as a decimal integer | [#209](https://github.com/Nimblesite/osprey/issues/209) | **closed** — rejected |
| C | Recovering a pointer from a word that never was one is unchecked | all | SIGSEGV, no diagnostic | — | **closed** — rejected |
| D | `print`/`toString` of an erased value reads through the erasure | all | Prints the address as a decimal integer | — | open — needs §8 |

Two *different* causes hide behind one symptom, which is why B and C could be
closed without touching the representation:

- **Recovery is a checker question.** B and C both take a value OUT of an `any`
  at an annotation. Nothing in the backend can make that safe, so inference
  refuses it (§7) and the unchecked cast is never reached. This needed no
  representation change and, contrary to the original sequencing, did **not**
  depend on splitting `LType`.
- **A and D are representational.** `LType::I64` is simultaneously every `int`,
  every erased `any`, and every *borrowed* `any` parameter, so neither an
  ownership rule nor a renderer can key off anything real. Both are blocked on
  the runtime descriptor in §8.

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

**Closed.** Not by making the two paths agree — by deleting both. `erasure_is_one_way`
(`crates/osprey-types/src/unify.rs`) rejects an `any` actual against a concrete
expected at every assignment site, so neither spelling reaches emission:

```
$ osprey c.osp --check
c.osp:2:3: function `viaReturn` body: cannot recover `string` from an erased `any`: match its structure instead
c.osp:4:0: let `viaLet`: cannot recover `string` from an erased `any`: match its structure instead
```

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

`139` is SIGSEGV. This was the concrete cost of "code that consumes its
representation must already know what was passed": every un-erasure was an
unchecked assertion, and the checker offered no way to make it a checked one.

**Closed** by the same rule as B — `useIt`'s annotation is rejected, so there is
no assertion left to be unchecked. `length(intish())` is a second spelling of
the same read and was already rejected by the builtin-constraint check
(``` `length` supports only string, List<T>, or Map<string, V>; got any ```).

## 4a. Finding D — rendering reads through the erasure

Found while confirming C. `print`/`toString` accept `any` as stand-in
polymorphism, so they consume the erased word with no evidence of its shape:

```osprey
type Point = { x: int, y: int }
fn erasedRecord() -> any = Point { x: 1, y: 2 }
fn erasedString() -> any = "a" + "b"
print("r=${erasedRecord()} s=${erasedString()}")
```

```
$ osprey g.osp --check
g.osp: ok (4 statements)
$ osprey g.osp --run --quiet --memory=default
r=4310245152 s=4310245216
```

Identical under `--memory=gc` and `--memory=arc`, and in both flavors. This is
B's symptom reached through a builtin rather than an annotation, so B's repair
does not touch it: the value is never assigned to a concrete type, it is
*rendered*. A scalar `any` renders correctly, which is why the corpus never
caught it. Rejecting all rendering would be truthful but would leave `any`
write-only; the descriptor in §8 makes it correct instead, so D is deliberately
left open rather than papered over with a narrower `print`.

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
fn forwardedHeap() -> string = processAny(forward("dyn" + "amic"))
```

The heap value is forwarded into a second `any` sink rather than recovered as a
`string`, because Phase 2 rejects the recovery. The property under test is
unchanged — the erasure must not disturb the producing frame's ownership — and
the ARC oracle, not a readable result, is what observes it until §8 lands.

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

## 8. The representation A and D need

Settled while closing B and C; recorded here so the remaining phases are an
implementation rather than a design.

**An erased `any` is a pointer to a two-word box**, `{ i8* desc, i64 payload }`,
where `desc` points at a module-global `{ i64 kind, i64 nfields, [i8*] names }`.
Scalars share one static descriptor per kind; each erased row shape gets one
naming its fields in layout order, so a field's slot is implied by its index
into the existing `{ i64 tag, fields… }` block and needs no offset table.

Three properties follow, and each is what one finding needs:

- **Ownership becomes uniform** (A). The box is an ordinary `osp_alloc_tagged`
  allocation whose payload word is masked managed exactly when it holds a
  pointer, so ARC drops it through machinery that already exists. This is the
  one shape that escapes §6: an erasure (`Str`/`Ptr` → box) transfers, while an
  `any` → `any` passthrough copies a pointer and borrows, and `fn identity(x:
  any) -> any = x` therefore gains no fictitious owner.
- **Narrowing becomes a comparison** (Phase 3). Every erasure shape and every
  structural pattern is known at compile time, so the compiler computes each
  pattern's matching descriptor set and emits pointer-equality tests — no
  `strcmp`, no field-name lookup at runtime.
- **Rendering becomes truthful** (D). `print`/`toString` switch on `kind`.

**No C runtime change is required.** Boxing is `osp_alloc_tagged`, already a
hook every backend defines; descriptor reads are GEP + load. That keeps the
change inside codegen and off the four per-backend object lists in the Makefile.
Rendering is the one part that wants C, and `string_runtime.c` is already linked
by default, GC, ARC **and** wasm32.

---

## TODO

### Phase 2 — delete recovery-by-annotation (#209, finding C) — **done**

- [x] Reject an `any` actual against a concrete expected at every assignment
      site (`erasure_is_one_way`, `crates/osprey-types/src/unify.rs`). This
      replaced "remove the `inttoptr` in `coerce_return`": that cast still
      carries the uniform collection/fiber element ABI, and rejecting in
      inference is both stronger and narrower than editing it.
- [x] `examples/failscompilation/{,ml_}any_recovery_by_annotation.ospo` — return
      annotation, `let` annotation, and a scalar `any` recovered as a `string`,
      in both flavors, with byte-exact diagnostic goldens.
- [x] An abandoning handler arm may no longer answer an erased word
      ([EFFECTS-HANDLER-ARMS]); `an_abandoning_arm_may_not_answer_an_erased_word`
      pins it.
- [x] The corpus twins forward an erased HEAP value into another `any` sink
      instead of recovering it, so the ARC live-object oracle still observes the
      §5 case. 179/179 under default, GC and ARC, `TEST_CORPUS_ARC_LEAKY=0`.

### Phase 1 — the erased-value box (§8)

- [ ] `{ i8* desc, i64 payload }` box on erasure; one module-global descriptor
      per scalar kind and per erased row shape.
- [ ] Give an erased `any` its own `LType` distinct from `I64` so
      `coerce_return`, `Stmt::Let` and the effect mailbox can see the erasure.
- [ ] `slot_is_managed` (`crates/osprey-codegen/src/effect_mailbox.rs`)
      classifies an erased operand from the descriptor, not its ABI.
- [ ] `cross_flavor_ir_equiv` stays green.

### Phase 3 — structural narrowing, ownership and rendering (#208, D)

- [ ] `{ x }` / `{ x, .. }` / `(a, b)` patterns lower to descriptor-set
      comparisons, both flavors ([PATTERN-STRUCTURAL], [PATTERN-TUPLE]).
      `Pattern::Structural` already parses and type-checks; only codegen rejects
      it (`unsupported construct: destructuring match arm`). `..` is not yet in
      either grammar.
- [ ] Ownership keys off the descriptor, never the machine word (§6).
- [ ] `print`/`toString` switch on `kind` instead of printing the word (D).
- [ ] Corpus cases with `OSPREY_ARC_DEBUG=1`: erased value narrowed once and
      twice, through an effect operand, and discarded while still erased. Each
      asserts zero live objects.
- [ ] Restore the recovery assertions Phase 2 moved to `failscompilation` as
      structural narrowings — an erased heap string read back through a match.

### Phase 5 — verification

- [ ] `make ci` green; differential harness under all three backends **and**
      `OSPREY_TARGET=wasm32`. (wasm32 needs Node 24+; `node:wasi` in Node 22
      corrupts module memory after `memory.grow` and fails every case.)
- [ ] Re-run every probe in this plan and confirm the quoted output changed.
- [ ] Close #208 with the corpus case that pins it; #209 is closed by Phase 2.
- [ ] Delete this plan and its README row.
