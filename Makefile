# Build from the repository root. See docs/contributing/repository-map.md.
# This file holds the repository-wide targets. Every other public target lives
# in a folder Makefile, one folder per area, as Book/Makefile does: apps/ (every
# app), packages/, tests/hunt/, vscode-extension/, website/, benchmarks/ and
# examples/proof/. Each folder target runs the private `_` target of the same
# name here; the included fragments hold the configuration and those steps. Keep `NAME := value` lines in the
# fragments: those panels list them as targets.
.DEFAULT_GOAL := build

include scripts/make/config.mk
include scripts/make/runtime.mk
include scripts/make/tests.mk
include scripts/make/proof.mk
include scripts/make/apps.mk
include scripts/make/extension.mk

include scripts/ios.mk
include scripts/android.mk

# =============================================================================
# Standard Targets
# =============================================================================

## build: C runtime archives + Rust workspace (release) + VSCode extension
build: _runtime _runtime_klee $(EXT_NODE_DEPS)
	@echo "==> Building..."
	cargo build --release --workspace
	cd $(EXT_DIR) && npm run compile

## audit: Full dependency vulnerability scan; scanner errors and advisories fail.
.PHONY: audit
audit:
	python3 hosts/gpui/dependencies.py
	python3 hosts/gpui/dependencies_test.py
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
	$(MAKE) target/osprey-pkg
	$(MAKE) _test_registry REGISTRY_MEMORY=gc
	$(MAKE) _test_registry REGISTRY_MEMORY=arc
	$(MAKE) _test_python_app_ports
	$(MAKE) _test_c_runtime
	$(MAKE) _test_hunt_mock
# The KLEE engine gates (_test_hunt, _test_hunt_crashes, _test_hunt_clean) are
# deliberately NOT here, as in CI: they cannot go green while issues #306 and
# #307 are open. Run them by hand on a machine with KLEE. [SYMEX-CRASH-GATE]
	$(MAKE) _coverage_check_c_runtime
# _test_language_corpus is deliberately NOT here. It runs every program in
# tests/ through the TAP subcommand, and _test_goldens then runs every one of
# them AGAIN — `run_test_corpus.sh` checks the in-language assertions AND the
# byte-exact golden in a single pass (see its header), so the golden run is
# strictly the stronger of the two observations. Executing the same assertions
# twice buys nothing and doubles the slowest stage in the build. The `osprey
# test` subcommand itself stays covered by bank-test and the CLI e2e suite.
	$(MAKE) _conformance-gc
	$(MAKE) _conformance-arc
	$(MAKE) _test_proof_kernel
	$(MAKE) _test_disciplines
	$(MAKE) _test_profiler
	$(MAKE) _vsix_bundle
	$(MAKE) _test_vscode_extension
	$(MAKE) _coverage_check_vscode_extension
	$(MAKE) _test_bench_tools

.PHONY: _package-coverage _test_packages _coverage_check_packages
_package-coverage: build
	$(MAKE) _test_packages
	$(MAKE) _coverage_check_packages

# [PKG-COVERAGE-GATE] One report per package, never averaged. _test_packages
# reruns all packages/<name>/tests suites concurrently with --coverage-json
# into $(COVERAGE_DIR)/packages; _coverage_check_packages gates the collected
# reports alone — no suite is re-run to check what it already measured — and
# fails on a package below its projects.packages threshold or on missing,
# empty, malformed or incomplete evidence.
_test_packages:
	python3 scripts/package_coverage.py --collect --compiler $(BIN) --reports $(COVERAGE_DIR)/packages --thresholds $(COVERAGE_THRESHOLDS_FILE)

_coverage_check_packages:
	python3 scripts/package_coverage.py --compiler $(BIN) --reports $(COVERAGE_DIR)/packages --thresholds $(COVERAGE_THRESHOLDS_FILE)

.PHONY: _python-app-ports _test_python_app_ports
_python-app-ports: build
	$(MAKE) _test_python_app_ports

_test_python_app_ports:
	python3 scripts/test-python-app-ports.py --compiler $(BIN)


_hunt: build
	./target/release/osprey hunt $(FILE) $(HUNT_ARGS)

.PHONY: _hunt-sample _hunt-crashes
_hunt-sample: build
	python3 scripts/hunt_sample.py $(HUNT_ARGS)

_hunt-crashes: build
	$(MAKE) _test_hunt_crashes

_hunt-corpus: build
	scripts/hunt_corpus.sh $(HUNT_ARGS)
	python3 scripts/hunt_summary.py target/hunt-corpus --findings-only

_registry-test: build
	$(MAKE) _test_registry

_registry-serve: target/osprey-registry target/local-registry.stamp
	@$(MKDIR) target/local-registry-server
	@printf '{ "root": "%s", "address": "127.0.0.1", "port": 8787 }\n' "$(abspath target/local-registry)" \
	  > target/local-registry-server/registry-server.json
	cd target/local-registry-server && $(abspath target/osprey-registry)

_registry-serve-sample: build
	cd packages/manager/api && $(abspath $(BIN)) . --run

_registry-blockers: build
	$(MAKE) _test_registry_blockers

_bank: _bank-web
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

_bank-web: build _runtime_wasm $(BANK_WEB_NODE_DEPS)
	@echo "==> Building Talon Bank browser application..."
	cd examples/projects/modules/web && npm run build

_bank-test: build
	$(MAKE) _bank_test

_bank-e2e: _bank-web $(BANK_E2E_NODE_DEPS)
	@echo "==> Bank e2e (Playwright)..."
	cd examples/projects/modules/e2e && npx playwright install chromium && npx playwright test
	@test ! -e /tmp/talon_bank.hold || { echo "FAIL: bank e2e left /tmp/talon_bank.hold behind; its webServer was stopped without running its trap"; exit 1; }

_docs-html-test: build $(BANK_E2E_NODE_DEPS)
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
ci: lint hawk test _bank-test _bank-e2e _docs-html-test _mobile-domain-test _bank-mobile-domain-test build

_wasm: build _runtime_wasm
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

_wasm _wasm-site _runtime_wasm _bank-web _test_wasm_goldens: export PATH := $(WASM_PATH_PREFIX)$(PATH)

_wasm-site: _runtime_wasm
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

_wasm-serve: _wasm
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
	node scripts/link-skills.mjs
	@echo "==> Setup complete. Run 'make ci' to validate."

## help: List every public target with its usage and expected result.
.PHONY: help
help:
	@grep -hE '^##' $(MAKEFILE_LIST) | sed 's/^## \{0,1\}//'
	@for folder in $(MAKE_FOLDERS); do echo; echo "make -C $$folder <target>:"; grep -hE '^##' $$folder/Makefile | sed 's/^## \{0,1\}/  /'; done

# =============================================================================
# Feature checks — each comment states the command's expected result
# =============================================================================

.PHONY: _proof
_proof: build
	for theorem in id_imp and_swap Nat.add_comm; do bash scripts/proof-export.sh --check --theorem $$theorem || exit 1; done
	node --test scripts/test-proof-export.mjs
	python3 scripts/test-proof-openai.py
	$(MAKE) -j3 _test_proof_ci

.PHONY: _workbench-proof-assets
_workbench-proof-assets:
	python3 scripts/workbench-proof-assets.py target/workbench-proof-assets
	python3 scripts/workbench-proof-assets.py target/workbench-proof-assets --check

.PHONY: _workbench-test
_workbench-test: build
	for memory in gc arc; do \
	  $(BIN) test apps/workbench/tests --memory=$$memory --filter "proof library file import displays its actual checked statement" || exit 1; \
	  $(BIN) test apps/workbench/tests --memory=$$memory --filter "imported id_imp exposes its actual statement and no assumptions" || exit 1; \
	done

.PHONY: _workbench-native-test
_workbench-native-test: build
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash apps/workbench/hosts/apple/run.sh --build
	swift test --package-path apps/workbench/hosts/apple

WORKBENCH_TREE ?= target/workbench-vsix/slate-board.native.json
.PHONY: _workbench-render-review
_workbench-render-review:
	OSPREY_RENDER_REVIEW_FULL_PAGE=1 bash apps/workbench/hosts/apple/render-review.sh $(WORKBENCH_TREE) 1488 1058 target/native-slate.png

## cli-contracts: Compiler CLI contracts: --quiet and --no-warnings (2 each),
##                closed-stdin launcher (3, OSPREY_TEST_COMPILER/OSPREY_TEST_MEMORY
##                forwarded), checked source graph (3), Windows DLL boundary (6).
.PHONY: cli-contracts
cli-contracts:
	cargo test -p osprey-cli --test cli_e2e quiet
	cargo test -p osprey-cli --test cli_e2e no_warnings
	cargo test --profile ci -p osprey-cli --test input_never_blocks
	cargo test --profile ci -p osprey-cli checked_source_graph
	cargo test -p osprey-cli windows_dll

.PHONY: _package-checks
_package-checks: build
	node scripts/verify-package-licenses.mjs
	node --test scripts/test-package-licenses.mjs
	python3 scripts/test-package-sha256.py
	$(BIN) packages/manager/fixture-check --run
	for filter in "schema-check: model declarations" "schema-check: accepted values" "schema-check: rejected values" \
	  "schema: spec block equals canonical file" "fixtures: names"; do \
	  $(BIN) test packages/manager/tests --filter "$$filter" || exit 1; \
	done
	for filter in "publish: licenses survive publication" "fetch: licenses and attribution are preserved"; do \
	  $(BIN) test packages/manager/cli/tests --filter "$$filter" || exit 1; \
	done
	for memory in gc arc; do \
	  $(BIN) test packages/manager/tests --memory=$$memory --filter "sha256: raw digest" || exit 1; \
	  $(BIN) test packages/manager/cli/tests --memory=$$memory --filter "add: partial" || exit 1; \
	  $(BIN) test packages/manager/cli/tests --memory=$$memory --filter "tree graph: diamond and identical leaf across orgs" || exit 1; \
	  $(BIN) test packages/manager/cli/tests --memory=$$memory --filter "reports: success output is not a finding" || exit 1; \
	  $(BIN) test packages/manager/editor/tests --memory=$$memory || exit 1; \
	  $(BIN) test packages/manager/editor/wire-tests --memory=$$memory || exit 1; \
	done
	$(BIN) packages/vscode/tests --run --quiet
	$(MAKE) target/package-workspace.wasm
	node $(EXT_DIR)/scripts/verify-package-workspace.mjs target/package-workspace.wasm

.PHONY: _package-coverage-record
_package-coverage-record:
	python3 scripts/package_coverage.py --record-measurements

.PHONY: _vsix-e2e
_vsix-e2e: $(EXT_NODE_DEPS)
	OSPREY_TEST_GREP='package |direct dependency reports|serialized per-project mutation|command registration|sidebar Manage Packages button|package workspace|compiled SDK renderer E2E|workbench document host wiring$(if $(WORKBENCH_VISUAL),|workbench VSIX browser|workbench VSIX visual comparison,)' npm test --prefix $(EXT_DIR)

.PHONY: _hunt-demo
_hunt-demo:
	$(MAKE) _hunt FILE=tests/hunt/crash.osp HUNT_ARGS="--input-bytes 1 --max-time 10 --out target/hunt-demo"

# =============================================================================
# Applications
# =============================================================================



_gpt2: build $(GPT2_DIR)/weights/model.safetensors $(GPT2_DIR)/weights/merges.txt
	@case "$(FLAVOR)" in default|python) ;; *) echo "gpt2 FLAVOR must be default or python" >&2; exit 1 ;; esac
	@printf '%s\n%s\n%s\n' "$(PROMPT)" "$(TOKENS)" "$(BACKEND)" | OSPREY_GPU=$(GPU) ./$(BIN) $(GPT2_GENERATOR) --run --memory=arc

_gpt2-train: build $(GPT2_DIR)/weights/model.safetensors $(GPT2_DIR)/weights/merges.txt
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
	sudo cp $(RTB)/libfiber_runtime*.a $(RTB)/libhttp_runtime*.a /usr/local/lib/
	@echo "==> installed osprey and runtime archives."

_bench: build
	@zsh benchmarks/run.sh $(BENCH_FILTER)

_bench-osprey: build
	@BENCH_PARTIAL=1 zsh benchmarks/run.sh $(BENCH_FILTER)

_vsix-rebuild-reinstall:
	$(MAKE) _vsix_clean
	$(MAKE) build
	$(MAKE) _vsix_bundle
	$(MAKE) _vsix_package
	$(MAKE) _vsix_install

# =============================================================================
# Apple and Android targets
# =============================================================================

_ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/ios/build.sh ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/ios/build.sh ios-sim

_ios-test: _ios
	$(MAKE) _test_ios

_macos: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/macos/run.sh --build
	for scene in kali opal character; do \
		OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/graphics/macos/run.sh --build $$scene || exit 1; \
	done

.PHONY: _run-mac-hd
_run-mac-hd: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/macos/run.sh

.PHONY: _gpui-flock _gpui-painting _gpui-probe _gpui-bench _gpui-compare _gpui-test _gpui-smoke _gpui-signal-flow _gpui-synth _casting
_gpui-flock: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/gpui/flock/run.sh

_gpui-painting: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/gpui/painting/run.sh

_gpui-signal-flow: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash packages/signal_flow/app/run.sh

_gpui-synth: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/synth/run.sh

_casting: _runtime_macos _runtime
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/casting/run.sh

_gpui-probe: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash hosts/gpui/run.sh

_gpui-bench: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/gpui/flock/run.sh --bench 20

_gpui-compare: _runtime_macos
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/gpui/painting/reference/compare.sh
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/gpui/painting/reference/compare.sh --draw

_gpui-test: _runtime_macos _runtime
	python3 hosts/gpui/dependencies.py
	python3 hosts/gpui/dependencies_test.py
	cargo build --release -p osprey-cli
	for suite in packages/native/tests packages/signal_flow/tests examples/synth/test examples/gpui/flock/test examples/gpui/painting/test; do \
		$(BIN) test $$suite || exit 1; \
	done
	$(MAKE) _gpui-smoke _gpui-self-test
	cd hosts/gpui && cargo test --release && cargo clippy --release -- -D warnings

# [UI-MACHINE-SELF-TEST] the scripts run in `_gpui-self-test`.
# Every desktop app started in the GPUI host without a window, its smoke.jsonl replayed through the
# real C boundary; the probe's has no script and taps its Add button. [GPUI-HOST-VERIFICATION]
_gpui-smoke:
	for app in hosts/gpui examples/gpui/flock examples/gpui/painting packages/signal_flow/app examples/synth examples/casting; do OSPREY_BIN="$(CURDIR)/$(BIN)" bash $$app/run.sh --smoke || exit 1; done

_macos-test: _macos _runtime
	$(MAKE) _test_macos

.PHONY: _mobile-ios _mobile-ios-test _mobile-domain-test _mobile-test
_mobile-ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	bash examples/mobile/ios/run.sh --build ios
	bash examples/mobile/ios/run.sh --build ios-sim

_mobile-ios-test: _mobile-ios
	OSPREY_IOS_SKIP_BUILD=1 bash examples/mobile/ios/run.sh --smoke

_mobile-domain-test: _runtime
	python3 scripts/test-mobile-tools.py
	cargo build --release -p osprey-cli
	$(BIN) examples/mobile/inbox/test --run

_mobile-test: _mobile-domain-test _mobile-ios-test _android-test

_android: _runtime_android
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/mobile/android/build.sh
# The goldens need the runtime archives `android` builds, so they are ordered by
# a recipe line rather than listed as a sibling prerequisite: under `make -j`
# siblings may run at once, and the corpus would race the archive it links.
_android-test: _android
	$(MAKE) _test_android_goldens
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/test-android.sh
	OSPREY_ANDROID_SKIP_BUILD=1 bash examples/mobile/android/run.sh --smoke

# Native Talon clients share the browser's Osprey application and live API.
.PHONY: _bank-mobile-domain-test _bank-android _bank-android-test _bank-ios _bank-ios-test _bank-mobile-test

_bank-mobile-domain-test: _runtime
	cargo build --release -p osprey-cli
	$(BIN) examples/projects/modules/mobile/app/test --run

_bank-android: _runtime_android
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_ANDROID_SKIP_RUNTIME=1 bash examples/projects/modules/mobile/android/build.sh

_bank-android-test: _bank-android _runtime
	OSPREY_ANDROID_SKIP_BUILD=1 python3 scripts/bank-mobile-test.py -- bash examples/projects/modules/mobile/android/test.sh

_bank-ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/projects/modules/mobile/ios/run.sh --build ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/projects/modules/mobile/ios/run.sh --build ios-sim

_bank-ios-test: _bank-ios _runtime
	OSPREY_IOS_SKIP_BUILD=1 python3 scripts/bank-mobile-test.py -- bash examples/projects/modules/mobile/ios/test.sh

_bank-mobile-test: _bank-mobile-domain-test
	$(MAKE) _bank-android-test
	$(MAKE) _bank-ios-test
