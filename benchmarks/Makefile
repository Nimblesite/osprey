# Cross-language benchmarks. Each target runs its recipe in the root Makefile, where it is the
# private `_` target of the same name; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ..

.PHONY: bench bench-osprey

## bench: Build, then run the cross-language performance benchmark suite
##        (Osprey vs Rust/C/OCaml/Haskell — CPU via hyperfine, peak memory via
##        /usr/bin/time). Absent toolchains are skipped. Informational only:
##        NOT part of `make ci`/`make test` (perf is noisy on shared runners).
##        Pass a name filter via BENCH_FILTER=<substr>; results in
##        benchmarks/results/results.html. Every run preserves unmeasured
##        values and publishes only validated replacements. See benchmarks/README.md.
bench:
	$(MAKE) -C $(ROOT) _bench

## bench-osprey: Re-time ONLY the Osprey columns — default, `--memory=arc`,
##        `--memory=gc` and wasm32 — and merge them into the results already on
##        disk. No other language is compiled, run or timed, so this is the fast
##        loop for a compiler/runtime change. Every non-Osprey measurement in
##        benchmarks/results/ survives byte-for-byte at the data-record level;
##        the Osprey re-run happens in a temp dir and is published only if every
##        case succeeded, so a broken build can never erase the recorded data.
##        Needs one prior `make bench` for the other languages' baseline.
##        Honours BENCH_FILTER=<substr>. See benchmarks/README.md.
bench-osprey:
	$(MAKE) -C $(ROOT) _bench-osprey
