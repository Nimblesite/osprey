# Proof road test, 2026-10-29: M0

The record plan 0041 asks for at each milestone: what was written, how long it took and every language or library gap that hurt.

## M0: a proof checks

`examples/proof/kernel/` is the Pi fragment of [PROOF-KERNEL-THEORY] in ML-flavor Osprey: levels, de Bruijn terms, an environment of constants, β/δ/ζ/η reduction, type inference and checking. `make _test_proof_kernel` runs it under GC, ARC and wasm32 against `expectedoutput`.

| Measure | Value |
|---|---|
| Osprey lines (six files) | 330 |
| Wall time to a first green run | about four hours, most of it the compiler gap below |
| Longest function | 19 lines |

Output:

```
checked: id_imp
rejected: id_imp_bad: type mismatch: `(fun (p : Prop) => (fun (h : p) => p))` has type `(∀ (p : Prop), (p → Prop))` but `(∀ (p : Prop), (p → p))` was expected
rejected: not_a_type: `(fun (p : Prop) => (fun (h : p) => h))` is not a type; its type is `(∀ (p : Prop), (p → p))`
```

### Gaps that hurt

1. **Positional constructors across files.** A union variant declared `App Term Term` in one file and applied `App f a` in another was rejected as "record construction requires named fields": the applying file's parser never saw the declaration, and the resolver only knew the named-field form. Fixed in this milestone: the resolver folds saturated applications of imported positional variants after linking, the same fold the parser applies within one file ([TYPE-UNION-POSITIONAL]). This blocked the kernel entirely until fixed.
2. **Recursive functions need a signature for codegen.** Every recursive function whose return type is not inferred by the time codegen runs must carry a written signature. The checker then flags the ones it could have inferred as redundant, so the author adds signatures and removes most of them again. A kernel is almost entirely mutually recursive functions; this is a real cost and an inference bug, tracked for the compiler.
3. **No nested patterns.** `match (left, right)` on two constructors, or `App (Lam ...) arg`, is rejected, so every two-sided comparison is two functions (`normal` then `sortEq`, `varEq`, ... in `reduce.ospml`). Readable, but definitional equality in Lean's kernel is one function with a dozen cases.
4. **Reserved word `type`.** The natural field name for a declaration's type is reserved; the kernel writes `ty`.
5. **No Lean on this machine.** `benchmarks/proof/run.sh` is written and exits red without `lean`, so the M0 speed gate has no baseline yet. Installing elan in CI is the next step before the gate can run.

### What did not hurt

`Result` with a string error channel, `Map<string, Decl>`, `satAdd`/`satSub`, string interpolation for printing terms, and the ML layout all read well. The whole kernel is pure; no `Cell`, no `Array`, no bigint were needed at M0.
