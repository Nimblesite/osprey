# Build from the repository root. See docs/contributing/repository-map.md.
# This file holds the repository-wide targets. Every other public target lives
# in a folder Makefile, one folder per area, as docs/book/Makefile does: the platform folders macos/, windows/, ios/,
# android/ and web/ (every app, its tests in <platform>/tests/), tests/ (host-free), llms/, src/osprey/packages/, src/osprey/tests/hunt/, src/toolchain/vscode-extension/, src/toolchain/website/, src/toolchain/benchmarks/ and
# src/osprey/examples/proof/. A folder Makefile defines each of its targets once and runs it
# from here through scripts/make/forward.mk; the included fragments hold the configuration and shared private steps. Keep `NAME := value` lines in the
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

## build: C runtime archives + Rust workspace (release) + VSCode and Zed extensions
build: _runtime _runtime_klee $(EXT_NODE_DEPS)
	@echo "==> Building..."
	cargo build --manifest-path src/toolchain/Cargo.toml --release --workspace
	$(MAKE) _zed_build
	cd $(EXT_DIR) && npm run compile

## audit: Full dependency vulnerability scan; scanner errors and advisories fail.
.PHONY: audit
audit:
	python3 src/toolchain/hosts/gpui/dependencies.py
	python3 src/toolchain/hosts/gpui/dependencies_test.py
	osv-scanner scan source --recursive --all-vulns .

## test: Fail-fast tests + coverage + per-project threshold enforcement.
##       Projects listed in coverage-thresholds.json are each tested + checked.
test: build _build_c_runtime_tests
	@echo "==> Testing (fail-fast + coverage + per-project thresholds)..."
	$(MAKE) _test_runtime_incremental
	$(MAKE) _test_rust
	$(MAKE) _coverage_check_rust
	$(MAKE) _check_programs
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
# test` subcommand itself stays covered by `make -C src/toolchain/targets/tests bank` and the CLI e2e suite.
	$(MAKE) _conformance-gc
	$(MAKE) _conformance-arc
	$(MAKE) _test_proof_kernel
	$(MAKE) _test_disciplines
	$(MAKE) _test_profiler
	$(MAKE) _vsix_bundle
	$(MAKE) _test_vscode_extension
	$(MAKE) _test_zed_stdio
	$(MAKE) _coverage_check_vscode_extension
	$(MAKE) _test_bench_tools


## lint: Run all linters/analyzers (read-only). Checks formatting but does
## NOT rewrite it — `make fmt` does that. The fmt check lives HERE because
## `lint` is what the required CI job runs; a format gate only in an optional
## job cannot block a merge.
lint: _deslop _lint

## fmt: Format all code in-place. Pass CHECK=1 for read-only check (CI use).
fmt: $(EXT_NODE_DEPS)
	@echo "==> Formatting$(if $(CHECK), (check mode),)..."
	cargo fmt --manifest-path src/toolchain/Cargo.toml --all$(if $(CHECK), --check,)
	cd $(EXT_DIR) && npx prettier$(if $(CHECK), --check, --write) .

## media: Copy the published videos, posters and captions from the Videos checkout (../Videos, or VIDEOS_DIR=<path>) into their paths here, then commit them; prints "media: copied N files".
.PHONY: media
media:
	@scripts/media/copy.sh

## website: Clean, build and serve the website at http://localhost:8080 (WEBSITE_PORT=<n> to change; Ctrl-C stops it).
WEBSITE_PORT ?= 8080
.PHONY: website
website: media $(WEBSITE_NODE_DEPS)
	cd src/toolchain/website && npm run clean && npm run build && node tests/serve.cjs $(WEBSITE_PORT)

## clean: Remove all build artifacts
clean:
	@echo "==> Cleaning..."
	cargo clean --manifest-path src/toolchain/Cargo.toml
	$(RM) $(RTB) src/toolchain/compiler/lib
	cd $(EXT_DIR) && $(RM) out dist coverage test.log

## ci: lint + test + build + hawk + the bank, docs and inbox suites (full CI simulation)
ci: lint test build
	$(MAKE) -C src/toolchain hawk
	$(MAKE) -C src/toolchain/targets/tests bank
	$(MAKE) -C src/toolchain/targets/web/tests bank
	$(MAKE) -C src/toolchain/website docs-html-test
	$(MAKE) -C src/toolchain/targets/tests inbox
	$(MAKE) -C src/toolchain/targets/tests bank-mobile

_runtime_wasm _test_wasm_goldens: export PATH := $(WASM_PATH_PREFIX)$(PATH)

## preflight: Every check the PR workflows run (each run: step and the emulator script, read from ci.yml and the workflows it calls), each run even after one fails, coverage gates last; prints a PASS/FAIL/SKIP table (also target/preflight/summary.md) and fails if any check failed. ONLY=<regex> runs a subset.
.PHONY: preflight
preflight:
	node scripts/ci/preflight.mjs $(if $(ONLY),--only='$(ONLY)')

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
