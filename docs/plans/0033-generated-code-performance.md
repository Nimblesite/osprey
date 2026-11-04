# Plan 0033: Generated-code performance

Contract: [spec 0037 arithmetic effects](../specs/language/0037-ArithmeticEffects.md), [performance gate](../specs/ecosystem/PerformanceGate.md)

## Prerequisites in place
The `rust-checked` column tooling is already wired: `src/toolchain/benchmarks/run.sh` (`LANGS` includes `rust-checked`, builds it with `-C overflow-checks=on`), `src/toolchain/benchmarks/report.py` (`ORDER`/`LABEL` include `Rust (checked)`), `src/toolchain/benchmarks/README.md` documents the column. No timings have been recorded for it yet.

## Phase 1: honest baseline
- [ ] Record `rust-checked` timings so the published table shows the like-for-like checked-Rust column. Run `make -C src/toolchain/benchmarks bench` (the full suite: `src/toolchain/benchmarks/run.sh` builds `rust-checked` with `-C overflow-checks=on`, then `merge_results.py` + `report.py` merge and render). Acceptance: `src/toolchain/benchmarks/results/results.json` lists `"rust-checked"` in `languages` and every benchmark case has a `rust-checked` cell with `status: "ok"`; the baked `src/toolchain/website/src/_includes/benchmarks-tables.html` contains a `Rust (checked)` column. Requires `rustc` on PATH (absent toolchains are skipped, not fatal).
