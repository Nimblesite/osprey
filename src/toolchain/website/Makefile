# The website and the WebAssembly builds it publishes. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

## video-metadata: Measure delivered MP4 durations for VideoObject and video sitemap metadata (requires ffprobe).
.PHONY: video-metadata
video-metadata:
	cd src/toolchain/website && node scripts/measure-video-metadata.mjs

wasm wasm-site: export PATH := $(WASM_PATH_PREFIX)$(PATH)

.PHONY: wasm wasm-site wasm-serve docs docs-html-test

## docs: Rebuild the compiler and regenerate built-in reference pages and ML twins.
##       Removed builtins are pruned; generated pages remain untracked.
docs:
	cd $(ROOT) && cargo build --manifest-path src/toolchain/Cargo.toml -p osprey-cli
	cd src/toolchain/website && OSPREY_BIN="$(abspath $(ROOT))/target/debug/osprey" npm run generate-docs

## wasm: Build everything for the WebAssembly target, ready to go — the wasm
## runtime archive (src/toolchain/compiler/bin/libosprey_runtime_wasm.a), the hello example,
## and Osprey Data Studio in BOTH flavors (studio.{osp,ospml} -> one byte-
## identical manifest that drives the SQLite dashboard in src/osprey/examples/wasm/
## index.html) — then validate them and smoke-run under Node's WASI, the browser
## WASI shim, with committed expected output for hello and both Studio flavors.
## Requires clang (wasm32 backend),
## wasm-ld and a WASI sysroot —
## `brew install lld wasi-libc` (macOS) or the wasi-sdk. See
## docs/specs/compiler/0022-WebAssemblyTarget.md.
wasm: build _runtime_wasm
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

## wasm-site: Build only the WebAssembly artifacts published by the website.
##      Used by GitHub Pages before `npm run build`; does not rely on checked-in
##      wasm binaries. Requires clang, wasm-ld, a WASI sysroot, and node.
wasm-site: _runtime_wasm
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

## docs-html-test: Browser acceptance for the HTML documentation export. Real
##                 Chromium over file:// — the strictest case, and the one a
##                 reader hits first — covering every theme, custom stylesheet
##                 ordering, user guides, search with no server behind it, the
##                 mobile disclosure and keyboard navigation. Reuses the bank
##                 e2e project's Playwright install rather than adding another.
docs-html-test: build $(BANK_E2E_NODE_DEPS)
	@echo "==> Documentation HTML browser acceptance..."
	cd src/osprey/examples/projects/modules/e2e && npx playwright install chromium
	node scripts/ci/verify-docs-html.mjs $(BIN)
endif
