# Proof, language and discipline conformance gates.

# Assertion-driven language corpus: recursively runs both *.test.osp and
# *.test.ospml suites. These inspect internal values rather than stdout goldens.
# Some loopback integration twins intentionally use the same fixed port so their
# Default and ML sources lower to identical IR; one worker prevents port races.
# _test_proof_kernel: the proof kernel of plan 0041 (src/osprey/examples/proof/kernel)
# checks `id_imp` and rejects the controls with the kernel's own reason,
# byte-exact against its golden, and its assertion suite (adversarial
# structural-equality and universe cases, every single-edit corruption of the
# recorded export asserted on term structure, never on rendering) passes,
# under GC, ARC and on wasm32 [PROOF-KERNEL]. No pipe hides a status: every
# run is written to a file and must exit zero; a golden run is then diffed,
# and a suite run's TAP summary must say `failed=0` on all three backends.
_test_proof_kernel: _test_proof_kernel_gc _test_proof_kernel_arc _test_proof_kernel_wasm

_test_proof_kernel_gc _test_proof_kernel_arc: _test_proof_kernel_%:
	@$(MKDIR) target/proof
	./$(BIN) src/osprey/examples/proof/kernel --run --memory=$* 2>/dev/null > target/proof/kernel.$*.txt
	diff target/proof/kernel.$*.txt src/osprey/examples/proof/kernel/expectedoutput
	cd src/osprey/examples/proof/export_check && ../../../../../$(BIN) . --run --memory=$* 2>/dev/null > ../../../../../target/proof/export-check.$*.txt
	diff target/proof/export-check.$*.txt src/osprey/examples/proof/export_check/expectedoutput
	./$(BIN) src/osprey/examples/proof/kernel/tests --run --memory=$* 2>/dev/null > target/proof/kernel-tests.$*.txt; status=$$?; tail -1 target/proof/kernel-tests.$*.txt; test $$status -eq 0
	grep -q "failed=0" target/proof/kernel-tests.$*.txt

_test_proof_kernel_wasm:
	@$(MKDIR) target/proof
	./$(BIN) src/osprey/examples/proof/kernel --target=wasm32 --compile -o target/proof/kernel.wasm >/dev/null 2>&1
	node scripts/wasm/wasm-smoke.mjs target/proof/kernel.wasm src/osprey/examples/proof/kernel/expectedoutput
	./$(BIN) src/osprey/examples/proof/kernel/tests --target=wasm32 --compile -o target/proof/kernel-tests.wasm >/dev/null 2>&1
	node scripts/wasm/wasm-smoke.mjs target/proof/kernel-tests.wasm > target/proof/kernel-tests.wasm.txt
	grep -q "failed=0" target/proof/kernel-tests.wasm.txt

# _test_disciplines_passthrough: the `passthrough` reference discipline of spec
# 0050 ([TYPES-DISCIPLINE-REFERENCE]), run by `_test_disciplines` below. Its
# suite proves the marker rewrite, the stderr-to-diagnostic parser and the exact
# protocol JSON; the round trip proves an `hm` module importing a disciplined one
# and the reverse, byte-exact against its golden under GC, ARC and wasm32; the
# same program with the marker removed prints the same bytes; and a planted type
# error proves the discipline is what rejected the module, because the
# diagnostic carries its prefix.
_test_disciplines_passthrough:
	@echo "==> [disciplines] passthrough checker suite..."
	./$(BIN) test src/osprey/examples/disciplines/passthrough
	@echo "==> [disciplines] passthrough round trip: GC, ARC, wasm32..."
	set -o pipefail && ./$(BIN) src/osprey/examples/disciplines/roundtrip --run --memory=gc 2>/dev/null | diff - src/osprey/examples/disciplines/roundtrip/expectedoutput
	set -o pipefail && ./$(BIN) src/osprey/examples/disciplines/roundtrip --run --memory=arc 2>/dev/null | diff - src/osprey/examples/disciplines/roundtrip/expectedoutput
	@$(MKDIR) target/disciplines
	./$(BIN) src/osprey/examples/disciplines/roundtrip --target=wasm32 --compile -o target/disciplines/roundtrip.wasm >/dev/null 2>&1
	node scripts/wasm/wasm-smoke.mjs target/disciplines/roundtrip.wasm src/osprey/examples/disciplines/roundtrip/expectedoutput
	@echo "==> [disciplines] the same program under hm alone (marker removed)..."
	@rm -rf target/disciplines/roundtrip-hm && $(MKDIR) target/disciplines/roundtrip-hm/src
	@cp src/osprey/examples/disciplines/roundtrip/src/main.ospml src/osprey/examples/disciplines/roundtrip/src/numbers.ospml target/disciplines/roundtrip-hm/src/
	@grep -v 'osprey: types=passthrough' src/osprey/examples/disciplines/roundtrip/src/count.ospml > target/disciplines/roundtrip-hm/src/count.ospml
	@grep -v '@osprey/passthrough' src/osprey/examples/disciplines/roundtrip/osprey.toml > target/disciplines/roundtrip-hm/osprey.toml
	set -o pipefail && ./$(BIN) target/disciplines/roundtrip-hm --run --memory=gc 2>/dev/null | diff - src/osprey/examples/disciplines/roundtrip/expectedoutput
	@echo "==> [disciplines] the checker is what rejected a broken module..."
	@rm -rf target/disciplines/roundtrip-broken && $(MKDIR) target/disciplines/roundtrip-broken/src
	@cp src/osprey/examples/disciplines/roundtrip/src/main.ospml src/osprey/examples/disciplines/roundtrip/src/numbers.ospml src/osprey/examples/disciplines/roundtrip/src/count.ospml target/disciplines/roundtrip-broken/src/
	@printf '    export broken () = 1 + "x"\n' >> target/disciplines/roundtrip-broken/src/count.ospml
	@sed 's|"../passthrough"|"../../../src/osprey/examples/disciplines/passthrough"|' src/osprey/examples/disciplines/roundtrip/osprey.toml > target/disciplines/roundtrip-broken/osprey.toml
	@if ./$(BIN) target/disciplines/roundtrip-broken --check > target/disciplines/broken.txt 2>&1; then \
	  echo "FAIL: the module with a planted type error was accepted"; cat target/disciplines/broken.txt; exit 1; fi
	@grep -q 'passthrough/check: type mismatch' target/disciplines/broken.txt
	@cat target/disciplines/broken.txt
	@$(RM) src/osprey/examples/disciplines/roundtrip/.osprey src/osprey/examples/disciplines/passthrough/.osprey

_test_language_corpus:
	@echo "==> [language] Default + ML assertion corpus..."
	@OSPREY_TEST_JOBS=1 ./$(BIN) test src/osprey/tests

# _test_goldens: byte-exact stdout comparison against the .expectedoutput
# goldens. Distinct from the target above, which runs the same corpus through
# the built-in TAP runner and so only ever observes whether the in-language
# assertions passed. An assertion can only state what someone thought to write
# down as a value; the goldens pin the ENTIRE observable output — print ORDER,
# exact formatting, and the TAP tally that exposes a deleted assertion.
_test_goldens: _conformance-gc

# _test_wasm_goldens: the same corpus, the same goldens, the OTHER code
# generator. Every program that links on wasm32 is compiled to a module, run
# under Node's WASI host, and held to the byte-exact output the native backend
# produces; the rest report as named skips because they use a feature the
# portable runtime deliberately does not port [WASM-TARGET]. Without this the
# wasm target is gated by three hand-picked example programs, which cannot
# notice a backend that miscompiles arithmetic, strings, maps or pattern
# matching everywhere else.
_test_wasm_goldens:
	@echo "==> [wasm32] golden stdout comparison under WASI..."
	@WASM_CC="$(WASM_CC)" WASI_SYSROOT="$(WASI_SYSROOT)" bash scripts/wasm/test-wasm-arc-ownership.sh
	@OSPREY_CORPUS_PART=programs OSPREY_TARGET=wasm32 zsh src/toolchain/crates/run_test_corpus.sh

# _test_wasm_doctests: every package and corpus documentation example compiled
# to wasm32 and held to its stated output under Node's WASI host. It is its own
# CI job so the two halves of the wasm corpus run side by side. [DOC-DOCTEST-HARNESS]
_test_wasm_doctests:
	@echo "==> [wasm32] documentation examples under WASI..."
	@OSPREY_CORPUS_PART=doctests OSPREY_TARGET=wasm32 zsh src/toolchain/crates/run_test_corpus.sh

# _conformance-gc: run every assertion suite under the tracing GC backend.
_conformance-gc:
	@echo "==> [conformance] assertion corpus under --memory=gc..."
	@OSPREY_TEST_JOBS=1 zsh src/toolchain/crates/run_test_corpus.sh gc

# _conformance-arc: run every assertion suite under the Perceus ARC backend;
# every suite must pass and end with zero live ARC objects. OSPREY_ARC_DEBUG=1 makes
# memory_arc.c report its live count at exit, which is the only automatic check
# for the [GC-ARC-PERCEUS] zero-leak bar.
_conformance-arc:
	@echo "==> [conformance] assertion corpus under --memory=arc..."
	@OSPREY_ARC_DEBUG=1 OSPREY_TEST_JOBS=1 zsh src/toolchain/crates/run_test_corpus.sh arc

# _conformance-ownership: run every assertion suite under compile-time
# ownership. A program listed in OWNERSHIP_ACCEPTED.txt must match the shared
# native golden under AddressSanitizer with zero leaks; every other program
# must be refused by the checker by name.
_conformance-ownership:
	@echo "==> [conformance] assertion corpus under --memory=ownership..."
	@OSPREY_CC="$(CURDIR)/scripts/ci/ownership-asan-clang.sh" OSPREY_SCRIBBLE_FREES=1 OSPREY_TEST_JOBS=1 zsh src/toolchain/crates/run_test_corpus.sh ownership

# The certificate the rejected `leanexport` module cites: Lean's real `Nat.add_comm`
# export with its proof replaced by its own statement, the same corruption the
# kernel's `natAddCommBad` fixture makes. Forged at test time, never committed.
LEAN_TAMPER = sed 's/"name":235,"type":730,"value":817/"name":235,"type":730,"value":730/' src/osprey/examples/proof/kernel/exports/nat_add_comm.ndjson > target/disciplines/lean_roundtrip/tampered.ndjson

# discipline_roundtrip: one reference discipline proved through the whole seam
# (checker build, protocol, erasure, signature ascription), not just its
# checker. $(1) is the package, $(2) its round-trip program, $(3) the module the
# program's `src/bad.ospml.rejected` replaces, and $(4) an optional command run
# in the scratch copy first (to forge an input the rejected module cites). Every leg is byte-exact: the
# program's stdout against its golden under GC and ARC, the compiled wasm
# module against the same golden under WASI, and the rejected variant against
# the compiler's own stderr. The scratch copy points its dependency back at the
# package in the checkout, and `sed` drops the absolute prefix so the golden
# holds no checkout path.
define discipline_roundtrip
	@echo "==> [disciplines] $(1): GC, ARC, wasm32 against expectedoutput..."
	set -o pipefail && ./$(BIN) src/osprey/examples/disciplines/$(2) --run --memory=gc 2>/dev/null | diff - src/osprey/examples/disciplines/$(2)/expectedoutput
	set -o pipefail && ./$(BIN) src/osprey/examples/disciplines/$(2) --run --memory=arc 2>/dev/null | diff - src/osprey/examples/disciplines/$(2)/expectedoutput
	@$(MKDIR) target/disciplines
	./$(BIN) src/osprey/examples/disciplines/$(2) --target=wasm32 --compile -o target/disciplines/$(1).wasm >/dev/null 2>&1
	node scripts/wasm/wasm-smoke.mjs target/disciplines/$(1).wasm src/osprey/examples/disciplines/$(2)/expectedoutput
	@echo "==> [disciplines] $(1): the rejected variant is refused..."
	@$(RM) target/disciplines/$(2)
	cp -R src/osprey/examples/disciplines/$(2) target/disciplines/$(2)
	cp target/disciplines/$(2)/src/bad.ospml.rejected target/disciplines/$(2)/src/$(3)
	$(RM) target/disciplines/$(2)/src/bad.ospml.rejected target/disciplines/$(2)/expectedoutput target/disciplines/$(2)/.osprey
	sed 's|"../$(1)"|"../../../src/osprey/examples/disciplines/$(1)"|' src/osprey/examples/disciplines/$(2)/osprey.toml > target/disciplines/$(2)/osprey.toml
	$(if $(4),$(4),@true)
	@./$(BIN) target/disciplines/$(2) --check > target/disciplines/$(1)-rejected.txt 2>&1; test $$? -ne 0
	sed -e "s|$(CURDIR)/||g" target/disciplines/$(1)-rejected.txt | diff - src/osprey/examples/disciplines/$(2)/rejected.expectedoutput
	@$(RM) src/osprey/examples/disciplines/$(1)/.osprey src/osprey/examples/disciplines/$(2)/.osprey
endef

# _test_disciplines: the reference type disciplines of [TYPES-DISCIPLINE-REFERENCE]
# (spec 0050). `bounded` reads `// @bound p in 0..100`, holds every call site
# inside the module to it by interval arithmetic, and erases to plain `int`.
# `dependent` checks the `theorem` lines of a module with the proof kernel and
# erases them; `leanexport` re-checks the pinned Lean certificates a module cites with
# the same kernel ([PROOF-DISCIPLINE]). Each round trip links the erased module into
# an `hm` program. The bounded checker's assertion suite then drives its built
# binary through the protocol, and `passthrough` proves the identity discipline.
_test_disciplines:
	$(call discipline_roundtrip,bounded,bounded_roundtrip,percent.ospml)
	$(call discipline_roundtrip,dependent,dependent_roundtrip,access.ospml)
	$(call discipline_roundtrip,lean,lean_roundtrip,tally.ospml,$(LEAN_TAMPER))
	@echo "==> [disciplines] bounded: the checker's protocol assertion suite..."
	@$(RM) src/osprey/examples/disciplines/bounded/.osprey
	@OSPREY_BIN="$(abspath $(BIN))" ./$(BIN) test src/osprey/examples/disciplines/bounded --memory=gc 2>/dev/null > target/disciplines/bounded-suite.txt; status=$$?; tail -2 target/disciplines/bounded-suite.txt; exit $$status
	grep -q "failed=0" target/disciplines/bounded-suite.txt
	@$(RM) src/osprey/examples/disciplines/bounded/.osprey
	$(MAKE) _test_disciplines_passthrough

.PHONY: _test_proof_kernel _test_proof_kernel_gc _test_proof_kernel_arc _test_proof_kernel_wasm
.PHONY: _test_disciplines _test_disciplines_passthrough

# Fresh checkers are shared read-only by the complete corpus checks. Make's
# jobserver bounds compilation and checking together; speed runs in isolation.
PROOF_BATCH_TARGETS := _test_proof_batch_gc _test_proof_batch_arc _test_proof_batch_wasm
PROOF_CORPUS_TARGETS := _test_proof_openai _test_proof_lean_core _test_proof_unicode

_test_proof_ci: _test_proof_kernel _test_disciplines _test_ownership_proof $(PROOF_CORPUS_TARGETS)
	OSPREY_BIN="$(abspath $(BIN))" zsh src/toolchain/benchmarks/proof/run.sh

# Verify single ownership and lender liveness along accepted ledger paths.
.PHONY: _test_ownership_proof
_test_ownership_proof: $(PROOF_BATCH_TARGETS)
	bash scripts/proof/ownership-ledger.sh
	bash scripts/proof/ownership-lowering.sh "$(abspath $(BIN))"

_test_proof_batch_gc _test_proof_batch_arc: _test_proof_batch_%:
	@$(MKDIR) target/proof/openai
	./$(BIN) src/osprey/examples/proof/batch --compile --quiet --no-warnings --memory=$* -o target/proof/openai/check-batch.$*

_test_proof_batch_wasm:
	@$(MKDIR) target/proof/openai
	./$(BIN) src/osprey/examples/proof/batch --compile --quiet --no-warnings --target=wasm32 -o target/proof/openai/check-batch.wasm

$(PROOF_CORPUS_TARGETS): $(PROOF_BATCH_TARGETS)

_test_proof_openai:
	python3 scripts/proof/proof-openai.py --compiler $(abspath $(BIN)) --reuse-binaries

_test_proof_lean_core:
	python3 scripts/proof/proof-openai.py --corpus lean-core --compiler $(abspath $(BIN)) --reuse-binaries

_test_proof_unicode:
	python3 scripts/proof/test-proof-literals.py

.PHONY: _test_proof_ci $(PROOF_BATCH_TARGETS) $(PROOF_CORPUS_TARGETS)

# [SYNTH-DSP-PROOFS] [CONCURRENCY-FIBER-LIFETIME] [CONCURRENCY-DEADLOCK] Check the current source
# exports with the Osprey kernel: every theorem must PASS with no axioms.
.PHONY: _test_synthesis_proofs
SYNTHESIS_THEOREMS := weightedOverlapAdd travelingWavePropagation additivePeriod overlapNormalizationNecessary interpolatorFactor interpolatorEndpoints interpolatorConstant zeroPulseIndices hannEndpoints
SYNTHESIS_THEOREMS += allpassMagnitude allpassRecurrence antiderivativeAverage antiderivativeOffset antiderivativeConstantCurve hannQuarterEnergy hannEighthEnergy
SYNTHESIS_THEOREMS += radixTwoSplit radixTwoUpper realBinConjugate binAdditive binHomogeneous
FIBER_LIFETIME_THEOREMS := run_valid freed_iff freed_is_final spawn_then_both_release dead_list_conserves drained_releases_each_once
FIBER_LIFETIME_THEOREMS += channel_then_handle_release share_is_acquire free_is_release free_last_iff
# The FiberLifetime mutants: each edit breaks a claim, and its mark proves it applied.
FIBER_LEAK_EDIT := s/| .live 0, .release => some .freed/| .live 0, .release => some (.live 0)/
FIBER_LEAK_MARK := some (.live 0)
FIBER_LOST_EDIT := s/.drain => ⟨\[\], pending ++ released⟩/.drain => ⟨[], released⟩/
FIBER_LOST_MARK := .drain => ⟨[], released⟩
FIBER_EARLY_EDIT := s/| .held 0 => some .owned/| .held 0 => none/
FIBER_EARLY_MARK := | .held 0 => none
# [CONCURRENCY-DEADLOCK] A fiber the collector reports can never be woken.
FIBER_DEADLOCK_THEOREMS := step_keeps_known step_keeps_live step_keeps_stuck stuck_for_ever stuck_never_runs reachable_wait_is_live
# The mutant lets a fiber wake a waiter on a channel it cannot name.
FIBER_WAKE_EDIT := s/: Runs w g → Reach w g c → w.waits f c →/: Runs w g → w.waits f c →/
FIBER_WAKE_MARK := : Runs w g → w.waits f c →

_test_synthesis_proofs: $(PROOF_BATCH_TARGETS)
	$(MAKE) _check_synthesis_proofs

# $(call kernel_check_theorems,<Lean module>,<theorem names>): every theorem
# replays in the Osprey kernel built for gc, arc and wasm32, with no axioms.
define kernel_check_theorems
	cd src/toolchain/benchmarks/proof/tools && lake build lean4export $(1)
	@set -e; for name in $(2); do \
		theorem="$(1).$$name"; export_file="$(abspath target/proof)/$$theorem.ndjson"; \
		(cd src/toolchain/benchmarks/proof/tools && lake exe lean4export $(1) -- "$$theorem") > "$$export_file"; \
		for backend in gc arc wasm; do \
			if [ "$$backend" = wasm ]; then \
				result=$$(printf '%s\t%s\n' "$$theorem.ndjson" "$$theorem" | node --stack-size=4096 scripts/proof/proof-batch-wasm.mjs target/proof/openai/check-batch.wasm "$(abspath target/proof)"); \
			else \
				result=$$(printf '%s\t%s\n' "$$export_file" "$$theorem" | target/proof/openai/check-batch.$$backend); \
			fi; \
			echo "$$backend $$result"; test "$$result" = "PASS $$theorem | axioms: none"; \
		done; \
	done
endef

# $(call kernel_rejects_mutant,<Lean module>,<name>,<sed edit>,<grep needle>,<why it is wrong>):
# the module with one edit that breaks its claim must fail to check.
define kernel_rejects_mutant
	@sed '$(3)' src/toolchain/benchmarks/proof/tools/$(1).lean > "$(abspath target/proof)/$(1)_$(2).lean"
	@grep -qF '$(4)' "$(abspath target/proof)/$(1)_$(2).lean"
	@if (cd src/toolchain/benchmarks/proof/tools && lake env lean "$(abspath target/proof)/$(1)_$(2).lean") > /dev/null 2>&1; then \
		echo "$(1): the $(2) mutant was accepted ($(5))"; exit 1; fi
	@echo "REJECTED $(1) $(2) mutant ($(5))"
endef

.PHONY: _check_synthesis_proofs
_check_synthesis_proofs:
	$(call kernel_check_theorems,Synthesis,$(SYNTHESIS_THEOREMS))
	$(call kernel_check_theorems,FiberLifetime,$(FIBER_LIFETIME_THEOREMS))
	$(call kernel_rejects_mutant,FiberLifetime,leak,$(FIBER_LEAK_EDIT),$(FIBER_LEAK_MARK),the last release keeps the fiber - #359)
	$(call kernel_rejects_mutant,FiberLifetime,lost,$(FIBER_LOST_EDIT),$(FIBER_LOST_MARK),a drain loses the handles it took)
	$(call kernel_rejects_mutant,FiberLifetime,early,$(FIBER_EARLY_EDIT),$(FIBER_EARLY_MARK),a shared handle is freed under its last sharer)
	$(call kernel_check_theorems,FiberDeadlock,$(FIBER_DEADLOCK_THEOREMS))
	$(call kernel_rejects_mutant,FiberDeadlock,wake,$(FIBER_WAKE_EDIT),$(FIBER_WAKE_MARK),a fiber wakes a waiter on a channel it cannot name)
	@bash scripts/proof/production-fft.sh
