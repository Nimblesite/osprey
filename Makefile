# Build from the repository root. See docs/contributing/repository-map.md.
# This file holds the repository-wide targets. Every other public target lives
# in a folder Makefile, one folder per area, as docs/book/Makefile does: src/toolchain/targets/ (every
# app), src/osprey/packages/, src/osprey/tests/hunt/, src/toolchain/vscode-extension/, src/toolchain/website/, src/toolchain/benchmarks/ and
# src/osprey/examples/proof/. Each folder target runs the private `_` target of the same
# name here; the included fragments hold the configuration and those steps. Keep `NAME := value` lines in the
# fragments: those panels list them as targets.
.DEFAULT_GOAL := build

include scripts/make/config.mk
include scripts/make/runtime.mk
include scripts/make/tests.mk
include scripts/make/proof.mk
include scripts/make/apps.mk
include scripts/make/extension.mk

include scripts/make/ios.mk
include scripts/make/android.mk

# =============================================================================
# Standard Targets
# =============================================================================

## build: C runtime archives + Rust workspace (release) + VSCode extension
build: _runtime _runtime_klee $(EXT_NODE_DEPS)
	@echo "==> Building..."
	cargo build --manifest-path src/toolchain/Cargo.toml --release --workspace
	cd $(EXT_DIR) && npm run compile

## audit: Full dependency vulnerability scan; scanner errors and advisories fail.
.PHONY: audit
audit:
	python3 src/toolchain/hosts/gpui/dependencies.py
	python3 src/toolchain/hosts/gpui/dependencies_test.py
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
	$(MAKE) _test_agent
	$(MAKE) _test_c_runtime
	$(MAKE) _test_hunt_mock
# The KLEE engine gates (_test_hunt, _test_hunt_crashes, _test_hunt_clean) are
# deliberately NOT here, as in CI: they cannot go green while issues #306 and
# #307 are open. Run them by hand on a machine with KLEE. [SYMEX-CRASH-GATE]
	$(MAKE) _coverage_check_c_runtime
# _test_language_corpus is deliberately NOT here. It runs every program in
# src/osprey/tests/ through the TAP subcommand, and _test_goldens then runs every one of
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

.PHONY: _test_packages _coverage_check_packages
# [PACKAGE-WORKSPACE] Every package checks on its own, as a consumer and the
# editor see it. A package's tests list sibling source roots by hand, so they
# stay green when the package's own manifest cannot resolve its dependencies.
.PHONY: _check_packages
_check_packages:
	for manifest in src/osprey/packages/*/osprey.toml; do \
		$(BIN) $${manifest%/osprey.toml} --check --quiet --no-warnings || exit 1; \
	done

# [PKG-COVERAGE-GATE] One report per package, never averaged. _test_packages
# reruns all src/osprey/packages/<name>/tests suites concurrently with --coverage-json
# into $(COVERAGE_DIR)/packages; _coverage_check_packages gates the collected
# reports alone — no suite is re-run to check what it already measured — and
# fails on a package below its projects.packages threshold or on missing,
# empty, malformed or incomplete evidence.
_test_packages:
	python3 scripts/packages/package_coverage.py --collect --compiler $(BIN) --reports $(COVERAGE_DIR)/packages --thresholds $(COVERAGE_THRESHOLDS_FILE)

_coverage_check_packages:
	python3 scripts/packages/package_coverage.py --compiler $(BIN) --reports $(COVERAGE_DIR)/packages --thresholds $(COVERAGE_THRESHOLDS_FILE)

.PHONY: _test_agent 
# Osprey Agent's turn logic and real update path: no network and no window.
_test_agent:
	set -o pipefail && $(BIN) src/osprey/examples/agent/test --run --quiet --no-warnings | awk '{print; last = $$0} END {exit last !~ / 0 failed$$/}'
	$(BIN) src/osprey/examples/agent/test-app --run --quiet --no-warnings

.PHONY: _test_python_app_ports

_test_python_app_ports:
	python3 scripts/packages/test-python-app-ports.py --compiler $(BIN)


_hunt: build
	./target/release/osprey hunt $(FILE) $(HUNT_ARGS)

.PHONY: 

_bank-web: build _runtime_wasm $(BANK_WEB_NODE_DEPS)
	@echo "==> Building Talon Bank browser application..."
	cd src/osprey/examples/projects/modules/web && npm run build

_bank-test: build
	$(MAKE) _bank_test

_bank-e2e: _bank-web $(BANK_E2E_NODE_DEPS)
	@echo "==> Bank e2e (Playwright)..."
	cd src/osprey/examples/projects/modules/e2e && npx playwright install chromium && npx playwright test
	@test ! -e /tmp/talon_bank.hold || { echo "FAIL: bank e2e left /tmp/talon_bank.hold behind; its webServer was stopped without running its trap"; exit 1; }

_docs-html-test: build $(BANK_E2E_NODE_DEPS)
	@echo "==> Documentation HTML browser acceptance..."
	cd src/osprey/examples/projects/modules/e2e && npx playwright install chromium
	node scripts/ci/verify-docs-html.mjs $(BIN)

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
## in the tree. `scripts/ci/verify-no-dead-code.mjs` (run by `_lint`) answers the
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
	cd src/toolchain && RUSTC_BOOTSTRAP=1 cargo +$(HAWK_RUSTC) hawk check --only dead-public -D hawk::dead_public --target-dir $(CURDIR)/target/hawk

## fmt: Format all code in-place. Pass CHECK=1 for read-only check (CI use).
fmt: $(EXT_NODE_DEPS)
	@echo "==> Formatting$(if $(CHECK), (check mode),)..."
	cargo fmt --manifest-path src/toolchain/Cargo.toml --all$(if $(CHECK), --check,)
	cd $(EXT_DIR) && npx prettier$(if $(CHECK), --check, --write) .

## clean: Remove all build artifacts
clean:
	@echo "==> Cleaning..."
	cargo clean --manifest-path src/toolchain/Cargo.toml
	$(RM) $(RTB) src/toolchain/compiler/lib
	cd $(EXT_DIR) && $(RM) out dist coverage test.log

## ci: lint + hawk + test + bank-test + bank-e2e + build (full CI simulation)
ci: lint hawk test _bank-test _bank-e2e _docs-html-test _mobile-domain-test _bank-mobile-domain-test build

_wasm: build _runtime_wasm
	@echo "==> compiling the wasm example -> src/osprey/examples/wasm/build/"
	@$(MKDIR) src/osprey/examples/wasm/build
	$(BIN) src/osprey/examples/wasm/hello.osp --target=wasm32 --compile -o src/osprey/examples/wasm/build/hello.wasm
	@echo "==> validating + smoke-running src/osprey/examples/wasm/build/hello.wasm"
	$(call wasm_validate,src/osprey/examples/wasm/build/hello.wasm)
	node scripts/wasm/wasm-smoke.mjs         src/osprey/examples/wasm/build/hello.wasm src/osprey/examples/wasm/hello.expectedoutput
	node scripts/wasm/wasm-browser-smoke.mjs src/osprey/examples/wasm/build/hello.wasm src/osprey/examples/wasm/hello.expectedoutput
	@echo "==> compiling Osprey Data Studio (BOTH flavors) -> src/osprey/examples/wasm/build/"
	$(BIN) src/osprey/examples/wasm/studio.osp   --target=wasm32 --compile -o src/osprey/examples/wasm/build/studio.osp.wasm
	$(BIN) src/osprey/examples/wasm/studio.ospml --target=wasm32 --compile -o src/osprey/examples/wasm/build/studio.ospml.wasm
	$(call wasm_validate,src/osprey/examples/wasm/build/studio.osp.wasm src/osprey/examples/wasm/build/studio.ospml.wasm)
	@echo "==> both Studio flavors must emit the SAME manifest (byte-identical golden)"
	node scripts/wasm/wasm-smoke.mjs         src/osprey/examples/wasm/build/studio.osp.wasm   src/osprey/examples/wasm/studio.expectedoutput
	node scripts/wasm/wasm-browser-smoke.mjs src/osprey/examples/wasm/build/studio.osp.wasm   src/osprey/examples/wasm/studio.expectedoutput
	node scripts/wasm/wasm-smoke.mjs         src/osprey/examples/wasm/build/studio.ospml.wasm src/osprey/examples/wasm/studio.expectedoutput
	node scripts/wasm/wasm-browser-smoke.mjs src/osprey/examples/wasm/build/studio.ospml.wasm src/osprey/examples/wasm/studio.expectedoutput
	$(MAKE) _test_wasm_goldens
	@echo "==> wasm ready: built + validated + WASI/browser smoke green"

_wasm _wasm-site _runtime_wasm _bank-web _test_wasm_goldens: export PATH := $(WASM_PATH_PREFIX)$(PATH)

_wasm-site: _runtime_wasm
	@echo "==> building osprey compiler for the website wasm demo"
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	@echo "==> compiling Osprey Data Studio website assets -> src/osprey/examples/wasm/build/"
	@$(MKDIR) src/osprey/examples/wasm/build
	$(BIN) src/osprey/examples/wasm/studio.osp   --target=wasm32 --compile -o src/osprey/examples/wasm/build/studio.osp.wasm
	$(BIN) src/osprey/examples/wasm/studio.ospml --target=wasm32 --compile -o src/osprey/examples/wasm/build/studio.ospml.wasm
	$(call wasm_validate,src/osprey/examples/wasm/build/studio.osp.wasm src/osprey/examples/wasm/build/studio.ospml.wasm)
	node scripts/wasm/wasm-smoke.mjs         src/osprey/examples/wasm/build/studio.osp.wasm   src/osprey/examples/wasm/studio.expectedoutput
	node scripts/wasm/wasm-browser-smoke.mjs src/osprey/examples/wasm/build/studio.osp.wasm   src/osprey/examples/wasm/studio.expectedoutput
	node scripts/wasm/wasm-smoke.mjs         src/osprey/examples/wasm/build/studio.ospml.wasm src/osprey/examples/wasm/studio.expectedoutput
	node scripts/wasm/wasm-browser-smoke.mjs src/osprey/examples/wasm/build/studio.ospml.wasm src/osprey/examples/wasm/studio.expectedoutput
	@echo "==> website wasm demo ready"

## setup: Post-create dev environment setup (used by devcontainer)
setup: $(EXT_NODE_DEPS) $(WEBCOMPILER_NODE_DEPS) $(WEBSITE_NODE_DEPS)
	@echo "==> Setting up development environment..."
	rustup component add rustfmt clippy llvm-tools-preview
	command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov
	bash scripts/ci/install-deslop.sh
	node scripts/ci/link-skills.mjs
	@echo "==> Setup complete. Run 'make ci' to validate."

## help: List every public target with its usage and expected result.
.PHONY: help
help:
	@grep -hE '^##' $(MAKEFILE_LIST) | sed 's/^## \{0,1\}//'
	@for folder in $(MAKE_FOLDERS); do echo; echo "make -C $$folder <target>:"; grep -hE '^##' $$folder/Makefile | sed 's/^## \{0,1\}/  /'; done

# =============================================================================
# Feature checks — each comment states the command's expected result
# =============================================================================

.PHONY: _workbench-test
_workbench-test: build
	for memory in gc arc; do \
	  $(BIN) test src/osprey/workbench/tests --memory=$$memory --filter "proof library file import displays its actual checked statement" || exit 1; \
	  $(BIN) test src/osprey/workbench/tests --memory=$$memory --filter "imported id_imp exposes its actual statement and no assumptions" || exit 1; \
	done

.PHONY: _agent-self-test 
# [UI-MACHINE-SELF-TEST] Osprey Agent's whole UI on the SwiftUI host, the model a test double, against its goldens.
_agent-self-test: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	swift test --package-path src/toolchain/hosts/swift/Tests
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/agent/self-test.sh

WORKBENCH_TREE ?= target/workbench-vsix/slate-board.native.json

## cli-contracts: Compiler CLI contracts: --quiet and --no-warnings (2 each),
##                closed-stdin launcher (3, OSPREY_TEST_COMPILER/OSPREY_TEST_MEMORY
##                forwarded), checked source graph (3), Windows DLL boundary (6).
.PHONY: cli-contracts
cli-contracts:
	cargo test --manifest-path src/toolchain/Cargo.toml -p osprey-cli --test cli_e2e quiet
	cargo test --manifest-path src/toolchain/Cargo.toml -p osprey-cli --test cli_e2e no_warnings
	cargo test --manifest-path src/toolchain/Cargo.toml --profile ci -p osprey-cli --test input_never_blocks
	cargo test --manifest-path src/toolchain/Cargo.toml --profile ci -p osprey-cli checked_source_graph
	cargo test --manifest-path src/toolchain/Cargo.toml -p osprey-cli windows_dll

# =============================================================================
# Applications
# =============================================================================



## run: Compile and run an Osprey file (usage: make run FILE=<path>)
run: build
	@if [ -z "$(FILE)" ]; then echo "Usage: make run FILE=<path>"; exit 1; fi
	./$(BIN) $(FILE) --run

## install: Install osprey + runtime archives system-wide
install: build
	cargo install --path src/toolchain/crates/osprey-cli --force
	sudo $(MKDIR) /usr/local/lib
	sudo cp $(RTB)/libfiber_runtime*.a $(RTB)/libhttp_runtime*.a /usr/local/lib/
	@echo "==> installed osprey and runtime archives."

# =============================================================================
# Apple and Android targets
# =============================================================================

_ios: _runtime_ios _runtime_ios_sim
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/ios/build.sh ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/ios/build.sh ios-sim

_macos: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/macos/run.sh --build
	for scene in kali opal character; do \
		OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/graphics/macos/run.sh --build $$scene || exit 1; \
	done

.PHONY: _gpui-test _gpui-smoke 

_gpui-test: _runtime_macos _runtime
	python3 src/toolchain/hosts/gpui/dependencies.py
	python3 src/toolchain/hosts/gpui/dependencies_test.py
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	for suite in src/osprey/packages/native/tests src/osprey/packages/signal_flow/tests src/osprey/examples/synth/test src/osprey/examples/gpui/flock/test src/osprey/examples/gpui/painting/test; do \
		$(BIN) test $$suite || exit 1; \
	done
	$(MAKE) _gpui-smoke _gpui-self-test
	cd src/toolchain/hosts/gpui && cargo test --release && cargo clippy --release -- -D warnings

# Compile every app's archive once for the smoke and self-test targets. The synth wrapper bundles
# and opens the app after its inner run.sh returns, so compile its archive directly instead.
.PHONY: _gpui-apps
_gpui-apps:
	pids=; \
	for app in src/toolchain/hosts/gpui src/osprey/examples/gpui/flock src/osprey/examples/gpui/painting src/osprey/packages/signal_flow/app src/osprey/examples/casting; do \
		OSPREY_BIN="$(CURDIR)/$(BIN)" bash $$app/run.sh --compile & \
		pids="$$pids $$!"; \
	done; \
	OSPREY_GPUI_APP="$(CURDIR)/src/osprey/examples/synth" OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/toolchain/hosts/gpui/run.sh --compile & \
	pids="$$pids $$!"; \
	status=0; \
	for pid in $$pids; do wait $$pid || status=1; done; \
	exit $$status

# [UI-MACHINE-SELF-TEST] the scripts run in `_gpui-self-test`.
# Every desktop app started in the GPUI host without a window, its smoke.jsonl replayed through the
# real C boundary; the probe's has no script and taps its Add button. [GPUI-HOST-VERIFICATION]
_gpui-smoke: _gpui-apps
	for app in src/toolchain/hosts/gpui src/osprey/examples/gpui/flock src/osprey/examples/gpui/painting src/osprey/packages/signal_flow/app src/osprey/examples/synth src/osprey/examples/casting; do OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_GPUI_PREBUILT=1 bash $$app/run.sh --smoke || exit 1; done

.PHONY: _mobile-ios _mobile-ios-test _mobile-domain-test _mobile-test
_mobile-ios: _runtime_ios _runtime_ios_sim
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	bash src/osprey/examples/mobile/ios/run.sh --build ios
	bash src/osprey/examples/mobile/ios/run.sh --build ios-sim

_mobile-ios-test: _mobile-ios
	OSPREY_IOS_SKIP_BUILD=1 bash src/osprey/examples/mobile/ios/run.sh --smoke

_mobile-domain-test: _runtime
	python3 scripts/mobile/test-mobile-tools.py
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(BIN) src/osprey/examples/mobile/inbox/test --run

_mobile-test: _mobile-domain-test _mobile-ios-test _android-test

_android: _runtime_android
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/mobile/android/build.sh
# The goldens need the runtime archives `android` builds, so they are ordered by
# a recipe line rather than listed as a sibling prerequisite: under `make -j`
# siblings may run at once, and the corpus would race the archive it links.
_android-test: _android
	$(MAKE) _test_android_goldens
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/mobile/test-android.sh
	OSPREY_ANDROID_SKIP_BUILD=1 bash src/osprey/examples/mobile/android/run.sh --smoke

# Native Talon clients share the browser's Osprey application and live API.
.PHONY: _bank-mobile-domain-test _bank-android _bank-android-test _bank-ios _bank-ios-test _bank-mobile-test

_bank-mobile-domain-test: _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(BIN) src/osprey/examples/projects/modules/mobile/app/test --run

_bank-android: _runtime_android
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_ANDROID_SKIP_RUNTIME=1 bash src/osprey/examples/projects/modules/mobile/android/build.sh

_bank-android-test: _bank-android _runtime
	OSPREY_ANDROID_SKIP_BUILD=1 python3 scripts/mobile/bank-mobile-test.py -- bash src/osprey/examples/projects/modules/mobile/android/test.sh

_bank-ios: _runtime_ios _runtime_ios_sim
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/projects/modules/mobile/ios/run.sh --build ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/projects/modules/mobile/ios/run.sh --build ios-sim

_bank-ios-test: _bank-ios _runtime
	OSPREY_IOS_SKIP_BUILD=1 python3 scripts/mobile/bank-mobile-test.py -- bash src/osprey/examples/projects/modules/mobile/ios/test.sh

_bank-mobile-test: _bank-mobile-domain-test
	$(MAKE) _bank-android-test
	$(MAKE) _bank-ios-test
