# Build from the repository root. See docs/contributing/repository-map.md.
# Every public target is defined in this file, so `make` and editor task panels
# list them from one place; the included fragments hold the configuration and
# the private `_` steps behind them. Keep `NAME := value` lines in the
# fragments: those panels list them as targets.
.DEFAULT_GOAL := build

include scripts/make/config.mk
include scripts/make/runtime.mk
include scripts/make/tests.mk
include scripts/make/apps.mk
include scripts/make/extension.mk

include scripts/ios.mk
include scripts/android.mk

# =============================================================================
# Standard Targets
# =============================================================================

## build: C runtime archives + Rust workspace (release) + VSCode extension
build: _runtime _runtime_klee $(EXT_NODE_DEPS) $(PROFILER_UI_BUILT)
	@echo "==> Building..."
	cargo build --release --workspace
	cd $(EXT_DIR) && npm run compile

## audit: Full dependency vulnerability scan; scanner errors and advisories fail.
.PHONY: audit
audit:
	osv-scanner scan source --recursive --all-vulns .

## test: Fail-fast tests + coverage + per-project threshold enforcement.
##       Projects listed in coverage-thresholds.json are each tested + checked.
test: build
	@echo "==> Testing (fail-fast + coverage + per-project thresholds)..."
	$(MAKE) _test_runtime_incremental
	$(MAKE) _test_rust
	$(MAKE) _coverage_check_rust
	$(MAKE) _test_packages
	$(MAKE) _coverage_check_packages
	$(MAKE) _test_c_runtime
	$(MAKE) _test_hunt_mock
	$(MAKE) _test_hunt
	$(MAKE) _test_hunt_crashes
	$(MAKE) _coverage_check_c_runtime
# _test_language_corpus is deliberately NOT here. It runs every program in
# tests/ through the TAP subcommand, and _test_goldens then runs every one of
# them AGAIN — `run_test_corpus.sh` checks the in-language assertions AND the
# byte-exact golden in a single pass (see its header), so the golden run is
# strictly the stronger of the two observations. Executing the same assertions
# twice buys nothing and doubles the slowest stage in the build. The `osprey
# test` subcommand itself stays covered by bank-test and the CLI e2e suite.
	$(MAKE) _test_goldens
	$(MAKE) _conformance-gc
	$(MAKE) _conformance-arc
	$(MAKE) _test_profiler
	$(MAKE) _test_vscode_extension
	$(MAKE) _coverage_check_vscode_extension
	$(MAKE) _test_bench_tools

## package-coverage: per-package source coverage — collect, then gate all 18.
##                   Reruns each packages/<name>/tests suite with `osprey test
##                   --coverage-json` and enforces every threshold under
##                   projects.packages (90% floor, one report per package).
.PHONY: package-coverage _test_packages _coverage_check_packages
package-coverage: build
	$(MAKE) _test_packages
	$(MAKE) _coverage_check_packages

# [PKG-COVERAGE-GATE] One report per package, never averaged. _test_packages
# reruns every packages/<name>/tests suite with `osprey test --coverage-json`
# into $(COVERAGE_DIR)/packages; _coverage_check_packages gates the collected
# reports alone — no suite is re-run to check what it already measured — and
# fails on a package below its projects.packages threshold or on missing,
# empty, malformed or incomplete evidence.
_test_packages:
	python3 scripts/package_coverage.py --collect --compiler $(BIN) --reports $(COVERAGE_DIR)/packages --thresholds $(COVERAGE_THRESHOLDS_FILE)

_coverage_check_packages:
	python3 scripts/package_coverage.py --compiler $(BIN) --reports $(COVERAGE_DIR)/packages --thresholds $(COVERAGE_THRESHOLDS_FILE)


## hunt: Symbolic bug hunting with KLEE on one program [SYMEX-CLI]:
##       make hunt FILE=path/to/program.osp [HUNT_ARGS="--max-time 120"]
hunt: build
	./target/release/osprey hunt $(FILE) $(HUNT_ARGS)

## hunt-sample: Qualify all runnable apps for a fast corpus; hard 10-second cutoff.
.PHONY: hunt-sample hunt-crashes
hunt-sample: build
	python3 scripts/hunt_sample.py $(HUNT_ARGS)

## hunt-crashes: Detect and natively reproduce every deliberately planted crash.
hunt-crashes: build
	$(MAKE) _test_hunt_crashes

## hunt-corpus: Hunt every program under tests/ and examples/ not listed in
##              tests/HUNT_UNPORTABLE.txt, then print the table [SYMEX-CORPUS].
hunt-corpus: build
	scripts/hunt_corpus.sh $(HUNT_ARGS)
	python3 scripts/hunt_summary.py target/hunt-corpus --findings-only

## registry-test: Osprey package-system suites (packages/manager/tests) under the
##                packages/manager/core arithmetic policy. Excluded from `test` and
##                CI until the package system stabilises (maintainer decision).
registry-test: build
	$(MAKE) _test_registry

## registry-serve: Serve packages/manager/sample-registry over HTTP on
##                 127.0.0.1:8787 with the Osprey folder-registry server
##                 (settings in packages/manager/api/registry-server.json).
registry-serve: build
	cd packages/manager/api && $(abspath $(BIN)) . --run

## registry-blockers: Probe the host capabilities plan 0020 still needs
##                    (argv, exit status, binary-safe reads, ...). Expected to
##                    FAIL until each capability lands; never part of `test`.
registry-blockers: build
	$(MAKE) _test_registry_blockers

## bank: Rebuild and run the Talon Bank showcase for manual testing.
##       Opens http://127.0.0.1:18790 (dashboard) / /api/accounts (JSON API).
##       The hold marker keeps the server up; Ctrl-C removes it and exits.
bank: bank-web
	@echo "==> Talon Bank live on http://127.0.0.1:18790  (Ctrl-C to stop)"
	@touch /tmp/talon_bank.hold
	@trap 'rm -f /tmp/talon_bank.hold' EXIT INT TERM; \
	  (if command -v open >/dev/null && command -v curl >/dev/null; then \
	    attempts=0; \
	    until accounts="$$(curl -fsS http://127.0.0.1:18790/api/accounts 2>/dev/null)" && [[ "$$accounts" == *'"Priya Sharma"'* ]]; do \
	      attempts=$$((attempts + 1)); [ "$$attempts" -ge 200 ] && exit 0; sleep 0.1; \
	    done; \
	    open http://127.0.0.1:18790; \
	  fi) & \
	  ./$(BIN) examples/projects/modules --run

## bank-web: Regenerate the embedded React host + Osprey WebAssembly client.
##           Requires Node and a WASI sysroot; the generated Osprey Bundle is
##           committed so ordinary native/CI builds do not need either tool.
bank-web: build _runtime_wasm $(BANK_WEB_NODE_DEPS)
	@echo "==> Building Talon Bank browser application..."
	cd examples/projects/modules/web && npm run build

## bank-test: Native Osprey unit tests for the Talon Bank pure domain layer,
##            run through the built-in `osprey test` harness (TAP output).
bank-test: build
	$(MAKE) _bank_test

## bank-e2e: Browser end-to-end tests for the Talon Bank modules showcase
##           (examples/projects/modules) — real Chromium via Playwright drives
##           the compiled osprey binary serving its HTTP API and web UI.
##           The server's hold marker must not outlive the run: a leaked one
##           makes every later bank-*-test refuse to start.
bank-e2e: bank-web $(BANK_E2E_NODE_DEPS)
	@echo "==> Bank e2e (Playwright)..."
	cd examples/projects/modules/e2e && npx playwright install chromium && npx playwright test
	@test ! -e /tmp/talon_bank.hold || { echo "FAIL: bank e2e left /tmp/talon_bank.hold behind; its webServer was stopped without running its trap"; exit 1; }

## docs-html-test: Browser acceptance for the HTML documentation export. Real
##                 Chromium over file:// — the strictest case, and the one a
##                 reader hits first — covering every theme, custom stylesheet
##                 ordering, user guides, search with no server behind it, the
##                 mobile disclosure and keyboard navigation. Reuses the bank
##                 e2e project's Playwright install rather than adding another.
docs-html-test: build $(BANK_E2E_NODE_DEPS)
	@echo "==> Documentation HTML browser acceptance..."
	cd examples/projects/modules/e2e && npx playwright install chromium
	node scripts/verify-docs-html.mjs $(BIN)

## lint: Run all linters/analyzers (read-only). Checks formatting but does
## NOT rewrite it — `make fmt` does that. The fmt check lives HERE because
## `lint` is what the required CI job runs; a format gate only in an optional
## job cannot block a merge.
lint: _deslop _lint

## hawk: Dead-code gate (astral-sh/hawk). Fails the build when any `pub`
## declaration is unreachable from the osprey binary (hawk::dead_public).
##
## hawk counts the workspace's TEST binaries as reachability roots, so an item
## whose only callers are its own `#[cfg(test)]` module passes this gate — that
## is how `osprey_debug::DebugBuild` and `osprey_syntax::dependency_sets` lived
## in the tree. `scripts/verify-no-dead-code.mjs` (run by `_lint`) answers the
## narrower question "does PRODUCT code name this?" and catches that class.
## Neither gate subsumes the other; both run. Scoped
## to dead_public ONLY — unnecessary_public / restricted-visibility findings are
## over-exposure, not dead code, and several are irreducibly public for the
## integration tests under this workspace's `dead_code = "deny"` policy, so they
## must NOT fail the gate. hawk needs rustc_private (RUSTC_BOOTSTRAP=1) and its
## prebuilt driver is built against one exact rustc and refuses any other, so
## the gate runs hawk under that toolchain, not under whatever stable is today:
## CI installing the latest hawk over the latest stable failed the day stable
## moved to 1.99.1 while hawk 0.1.15 was built for 1.99.0. HAWK_VERSION and
## HAWK_RUSTC move together; `make hawk-install` installs both. An absent
## cargo-hawk FAILS this
## target: a gate that cannot run must not report success.
## Install: make hawk-install (https://github.com/astral-sh/hawk)
HAWK_VERSION ?= 0.1.15
HAWK_RUSTC   ?= 1.99.0
hawk-install:
	rustup toolchain install $(HAWK_RUSTC) --profile minimal
	curl --proto '=https' --tlsv1.2 -LsSf https://github.com/astral-sh/hawk/releases/download/$(HAWK_VERSION)/cargo-hawk-installer.sh | sh

hawk:
	@echo "==> Dead-code gate (hawk)..."
	@if ! command -v cargo-hawk >/dev/null 2>&1; then \
		echo "FAIL: cargo-hawk is not installed, so the dead-code gate cannot run."; \
		echo "      A gate that cannot run must not report success — this used to"; \
		echo "      print a warning and exit 0, so 'make hawk' was green on every"; \
		echo "      machine without the binary, the CI runner included if its"; \
		echo "      install step ever stopped landing cargo-hawk on PATH."; \
		echo "      Install: make hawk-install"; \
		exit 1; \
	fi
	RUSTC_BOOTSTRAP=1 cargo +$(HAWK_RUSTC) hawk check --only dead-public -D hawk::dead_public --target-dir $(CURDIR)/target/hawk

## fmt: Format all code in-place. Pass CHECK=1 for read-only check (CI use).
fmt: $(EXT_NODE_DEPS)
	@echo "==> Formatting$(if $(CHECK), (check mode),)..."
	cargo fmt --all$(if $(CHECK), --check,)
	cd $(EXT_DIR) && npx prettier$(if $(CHECK), --check, --write) .

## clean: Remove all build artifacts
clean:
	@echo "==> Cleaning..."
	cargo clean
	$(RM) $(RTB) compiler/lib outputs
	cd $(EXT_DIR) && $(RM) out dist coverage test.log

## ci: lint + hawk + test + bank-test + bank-e2e + build (full CI simulation)
ci: lint hawk test bank-test bank-e2e docs-html-test mobile-domain-test bank-mobile-domain-test build

## wasm: Build everything for the WebAssembly target, ready to go — the wasm
## runtime archive (compiler/bin/libosprey_runtime_wasm.a), the hello example,
## and Osprey Data Studio in BOTH flavors (studio.{osp,ospml} -> one byte-
## identical manifest that drives the SQLite dashboard in examples/wasm/
## index.html) — then validate them and smoke-run under Node's WASI, the browser
## WASI shim, with committed expected output for hello and both Studio flavors.
## Requires clang (wasm32 backend),
## wasm-ld and a WASI sysroot —
## `brew install lld wasi-libc` (macOS) or the wasi-sdk. See
## docs/specs/compiler/0022-WebAssemblyTarget.md.
wasm: build _runtime_wasm
	@echo "==> compiling the wasm example -> examples/wasm/build/"
	@$(MKDIR) examples/wasm/build
	$(BIN) examples/wasm/hello.osp --target=wasm32 --compile -o examples/wasm/build/hello.wasm
	@echo "==> validating + smoke-running examples/wasm/build/hello.wasm"
	$(call wasm_validate,examples/wasm/build/hello.wasm)
	node scripts/wasm-smoke.mjs         examples/wasm/build/hello.wasm examples/wasm/hello.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/hello.wasm examples/wasm/hello.expectedoutput
	@echo "==> compiling Osprey Data Studio (BOTH flavors) -> examples/wasm/build/"
	$(BIN) examples/wasm/studio.osp   --target=wasm32 --compile -o examples/wasm/build/studio.osp.wasm
	$(BIN) examples/wasm/studio.ospml --target=wasm32 --compile -o examples/wasm/build/studio.ospml.wasm
	$(call wasm_validate,examples/wasm/build/studio.osp.wasm examples/wasm/build/studio.ospml.wasm)
	@echo "==> both Studio flavors must emit the SAME manifest (byte-identical golden)"
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	$(MAKE) _test_wasm_goldens
	@echo "==> wasm ready: built + validated + WASI/browser smoke green"

wasm wasm-site _runtime_wasm bank-web _test_wasm_goldens: export PATH := $(WASM_PATH_PREFIX)$(PATH)

## wasm-site: Build only the WebAssembly artifacts published by the website.
##      Used by GitHub Pages before `npm run build`; does not rely on checked-in
##      wasm binaries. Requires clang, wasm-ld, a WASI sysroot, and node.
wasm-site: _runtime_wasm
	@echo "==> building osprey compiler for the website wasm demo"
	cargo build --release -p osprey-cli
	@echo "==> compiling Osprey Data Studio website assets -> examples/wasm/build/"
	@$(MKDIR) examples/wasm/build
	$(BIN) examples/wasm/studio.osp   --target=wasm32 --compile -o examples/wasm/build/studio.osp.wasm
	$(BIN) examples/wasm/studio.ospml --target=wasm32 --compile -o examples/wasm/build/studio.ospml.wasm
	$(call wasm_validate,examples/wasm/build/studio.osp.wasm examples/wasm/build/studio.ospml.wasm)
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	@echo "==> website wasm demo ready"

## wasm-serve: Build the wasm target (full `make wasm`), then static-host
##      $(WASM_SERVE_DIR) at http://localhost:$(WASM_SERVE_PORT)/ and open it in
##      your browser. Long-running dev server — Ctrl-C to stop. Override the port
##      with WASM_SERVE_PORT=<n>. (`make wasm` itself stays headless for CI.)
wasm-serve: wasm
	@URL="http://localhost:$(WASM_SERVE_PORT)/"; \
	  command -v python3 >/dev/null 2>&1 || { echo "FAIL: python3 not found (needed for the dev server)"; exit 1; }; \
	  echo "==> serving $(WASM_SERVE_DIR)/ at $$URL — opening browser (Ctrl-C to stop)"; \
	  OPENER=$$(command -v open || command -v xdg-open || true); \
	  if [ -n "$$OPENER" ]; then ( sleep 1; "$$OPENER" "$$URL" >/dev/null 2>&1 || true ) & \
	  else echo "  (no 'open'/'xdg-open' found — browse to $$URL manually)"; fi; \
	  cd $(WASM_SERVE_DIR) && exec python3 -m http.server $(WASM_SERVE_PORT)

## setup: Post-create dev environment setup (used by devcontainer)
setup: $(EXT_NODE_DEPS) $(WEBCOMPILER_NODE_DEPS) $(WEBSITE_NODE_DEPS)
	@echo "==> Setting up development environment..."
	rustup component add rustfmt clippy llvm-tools-preview
	command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov
	bash scripts/install-deslop.sh
	@echo "==> Setup complete. Run 'make ci' to validate."

# =============================================================================
# Applications
# =============================================================================

## graphics: Build the platform graphics host and run the animated GPU demo
#            Osprey drives the scene; the GPU shades every pixel. macOS renders
#            it through Metal in a Swift host linked to the Mac slice, Windows
#            through a Direct3D 12 DLL linked to the native executable — and the
#            Osprey sources are the same files on both, which is the entire point.
#            SCENE=kali|opal|character selects one of the three scenes; every
#            one is a one-file project sharing examples/graphics/base/base.osp
#            and one named fragment entry in the platform shader library.
#            GFX_WARN overrides the C warning set. See $(GFX_DIR)/README.md.
graphics: _graphics

## graphics-smoke: Render every scene for GFX_SMOKE_FRAMES frames and require
##                 each to report them. Needs a desktop session and a Metal GPU.
graphics-smoke: _graphics-smoke

## gpt2: Run GPT-2 (usage: make gpt2 FLAVOR=default|python PROMPT="..." TOKENS=20 BACKEND=gpu|lists)
## BACKEND=gpu runs the model on the GPU surface: every kernel and product on GPU=auto|metal|opencl, or on the CPU with GPU=host; BACKEND=lists runs the same model over host lists.
## The published weights and merges download once into examples/gpt2/weights (ignored by git).
gpt2: build $(GPT2_DIR)/weights/model.safetensors $(GPT2_DIR)/weights/merges.txt
	@case "$(FLAVOR)" in default|python) ;; *) echo "gpt2 FLAVOR must be default or python" >&2; exit 1 ;; esac
	@printf '%s\n%s\n%s\n' "$(PROMPT)" "$(TOKENS)" "$(BACKEND)" | OSPREY_GPU=$(GPU) ./$(BIN) $(GPT2_GENERATOR) --run --memory=arc

## gpt2-train: One training step of GPT-2 on the prompt: loss, gradients, SGD, loss again (usage: make gpt2-train PROMPT="..." BACKEND=gpu|lists)
gpt2-train: build $(GPT2_DIR)/weights/model.safetensors $(GPT2_DIR)/weights/merges.txt
	@test "$(FLAVOR)" = default || { echo "gpt2-train FLAVOR must be default" >&2; exit 1; }
	@printf '%s\n%s\n' "$(PROMPT)" "$(BACKEND)" | OSPREY_GPU=$(GPU) ./$(BIN) $(GPT2_DIR)/train --run --memory=arc

## run: Compile and run an Osprey file (usage: make run FILE=<path>)
run: build
	@if [ -z "$(FILE)" ]; then echo "Usage: make run FILE=<path>"; exit 1; fi
	./$(BIN) $(FILE) --run

## install: Install osprey + runtime archives system-wide
install: build
	cargo install --path crates/osprey-cli --force
	sudo $(MKDIR) /usr/local/lib
	sudo cp $(RTB)/libfiber_runtime.a $(RTB)/libhttp_runtime.a /usr/local/lib/
	@echo "==> installed osprey and runtime archives."

## bench: Build, then run the cross-language performance benchmark suite
##        (Osprey vs Rust/C/OCaml/Haskell — CPU via hyperfine, peak memory via
##        /usr/bin/time). Absent toolchains are skipped. Informational only:
##        NOT part of `make ci`/`make test` (perf is noisy on shared runners).
##        Pass a name filter via BENCH_FILTER=<substr>; results in
##        benchmarks/results/results.html. Every run preserves unmeasured
##        values and publishes only validated replacements. See benchmarks/README.md.
bench: build
	@zsh benchmarks/run.sh $(BENCH_FILTER)

## bench-osprey: Re-time ONLY the Osprey columns — default, `--memory=arc`,
##        `--memory=gc` and wasm32 — and merge them into the results already on
##        disk. No other language is compiled, run or timed, so this is the fast
##        loop for a compiler/runtime change. Every non-Osprey measurement in
##        benchmarks/results/ survives byte-for-byte at the data-record level;
##        the Osprey re-run happens in a temp dir and is published only if every
##        case succeeded, so a broken build can never erase the recorded data.
##        Needs one prior `make bench` for the other languages' baseline.
##        Honours BENCH_FILTER=<substr>. See benchmarks/README.md.
bench-osprey: build
	@BENCH_PARTIAL=1 zsh benchmarks/run.sh $(BENCH_FILTER)

## vsix-rebuild-reinstall: Rebuild the compiler and reinstall the Osprey VSCode
##      extension in place, bundling the freshly-built Rust binary as `osprey`.
##      Touches ONLY the Osprey extension ($(EXT_ID)) in the DEFAULT profile —
##      never another extension, never another VSCode profile. macOS only.
##      ONE `code` invocation (install --force, no separate uninstall) so the
##      running VSCode reconciles its extension host exactly once, not twice.
##      Every step is verified: the VSIX must contain the bundled compiler and
##      the installed extension must be byte-identical to this build, so
##      "installed" is a fact rather than a claim `code` printed.
vsix-rebuild-reinstall:
	$(MAKE) _vsix_clean
	$(MAKE) build
	$(MAKE) _vsix_bundle
	$(MAKE) _vsix_package
	$(MAKE) _vsix_install

# =============================================================================
# Apple and Android targets
# =============================================================================

## ios: Build the iPhone and simulator libraries and SwiftUI example apps.
##      Requires macOS, Xcode and both iOS SDKs. Device output is unsigned.
ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/ios/build.sh ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/ios/build.sh ios-sim

## ios-test: Validate C ABI calls, language goldens and the SwiftUI simulator app.
ios-test: ios
	$(MAKE) _test_ios

## macos: Build the Mac runtime, the compiler and every macOS example: the
##        storage app and the three graphics scenes, each a Swift host
##        linked to the Mac slice. Requires an Apple silicon Mac with Xcode.
##        [IOS-TARGET-MACOS]
macos: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/macos/run.sh --build
	for scene in kali opal character; do \
		OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/graphics/macos/run.sh --build $$scene || exit 1; \
	done

## run-mac-hd: Rebuild the compiler, the Mac runtime and the Osprey Storage
##             app, then open it: where your disk space goes, folder by folder.
.PHONY: run-mac-hd
run-mac-hd: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/macos/run.sh

## macos-test: The iPhone C ABI fixture, the whole corpus against the same
##             goldens and skip manifest, and the storage example, on this Mac.
##             The storage domain suite runs natively, so it needs `_runtime`.
macos-test: macos _runtime
	$(MAKE) _test_macos

.PHONY: mobile-ios mobile-ios-test mobile-domain-test mobile-test
mobile-ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	bash examples/mobile/ios/run.sh --build ios
	bash examples/mobile/ios/run.sh --build ios-sim

mobile-ios-test: mobile-ios
	OSPREY_IOS_SKIP_BUILD=1 bash examples/mobile/ios/run.sh --smoke

mobile-domain-test: _runtime
	python3 scripts/test-mobile-tools.py
	cargo build --release -p osprey-cli
	$(BIN) examples/mobile/inbox/test --run

mobile-test: mobile-domain-test mobile-ios-test android-test

android: _runtime_android
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/mobile/android/build.sh
# The goldens need the runtime archives `android` builds, so they are ordered by
# a recipe line rather than listed as a sibling prerequisite: under `make -j`
# siblings may run at once, and the corpus would race the archive it links.
android-test: android
	$(MAKE) _test_android_goldens
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/test-android.sh
	OSPREY_ANDROID_SKIP_BUILD=1 bash examples/mobile/android/run.sh --smoke

# Native Talon clients share the browser's Osprey application and live API.
.PHONY: bank-mobile-domain-test bank-android bank-android-test bank-ios bank-ios-test bank-mobile-test

bank-mobile-domain-test: _runtime
	cargo build --release -p osprey-cli
	$(BIN) examples/projects/modules/mobile/app/test --run

bank-android: _runtime_android
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_ANDROID_SKIP_RUNTIME=1 bash examples/projects/modules/mobile/android/build.sh

bank-android-test: bank-android _runtime
	OSPREY_ANDROID_SKIP_BUILD=1 python3 scripts/bank-mobile-test.py -- bash examples/projects/modules/mobile/android/test.sh

bank-ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/projects/modules/mobile/ios/run.sh --build ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/projects/modules/mobile/ios/run.sh --build ios-sim

bank-ios-test: bank-ios _runtime
	OSPREY_IOS_SKIP_BUILD=1 python3 scripts/bank-mobile-test.py -- bash examples/projects/modules/mobile/ios/test.sh

bank-mobile-test: bank-mobile-domain-test
	$(MAKE) bank-android-test
	$(MAKE) bank-ios-test
