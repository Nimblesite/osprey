# Performance Gate

**Status:** implemented; the cachegrind instruction gate and the optimisation oracle both run in CI.

## Performance gate [PERF-GATE]

Every optimisation lands with a test that asserts what LLVM did with the module, not how long the binary ran.

Wall-clock time is reported, never gated, on hosted runners. The gate uses a runner-independent measure.

The gate measures instruction counts, which do not depend on runner speed: `valgrind --tool=cachegrind` on Linux reports `Ir` for a binary deterministically to well under 1%, at roughly 50x slowdown, which on 10 ms to 100 ms cases is seconds. The job builds the Osprey and `c`/`rust-checked` binaries for a fixed subset of cases, records `Ir` for each, and fails when an Osprey/`rust-checked` ratio rises above the value in a thresholds file that only goes down, in the manner of `coverage-thresholds.json`. It is a required check with a job-level `if:`, never `on: paths:`.
