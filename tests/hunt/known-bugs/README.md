# Known bugs — excluded from the CI expectation suite

This directory documents known bugs. `tests/hunt/clean.osp` itself stays in `tests/hunt/` (checkpoint tooling owns that tree); `make _test_hunt` skips it by name via `HUNT_KNOWN_BUGS ?= clean` in `scripts/make/tests.mk`, because its **complete-exploration** expectation cannot be met inside the pinned budget. This is a performance defect, not a correctness one: the findings oracle is trivially satisfied; the gate fails on incomplete paths.

## `clean.osp` — GC and ARC

`clean.osp` prints a symbolic integer (`picked ${v} at ${i}`), and the digit/length queries over the unconstrained value exceed the 30-second engine budget: ~49 seconds of solver work on an idle Apple-silicon machine with the best measured engine flags (`--solver-backend=z3 --solver-optimize-divides`), which makes the pass flaky even locally and red on CI hardware. Under `--memory=gc` it is strictly worse (the input line's bytes reach the conservative scan as one expression web; see `docs/reports/klee-gc-status-2026-10-26.md`).

Plan item: `docs/plans/0034-symbolic-execution.md`, "Phase 3 completion proof" and "Phase 3 KNOWN BUG (GC backend)" — both unchecked. Fixing this is an open item; the 64-byte / 30-second settings are pinned by the plan and must not be relaxed.

Verify a fix with (expect all green):

```sh
make _test_hunt_clean
```

Profiled dead ends (measured neutral-to-worse, reverted): raw-view bounded parse, O(n) line copy, GC scan alignment filter, binary-search digit length. Evidence: `target/hunt-diag/`.
