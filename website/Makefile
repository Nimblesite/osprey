# The website and the WebAssembly builds it publishes. Each target runs its recipe in the root Makefile, where it is the
# private `_` target of the same name; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ..

.PHONY: wasm wasm-site wasm-serve docs-html-test

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
wasm:
	$(MAKE) -C $(ROOT) _wasm

## wasm-site: Build only the WebAssembly artifacts published by the website.
##      Used by GitHub Pages before `npm run build`; does not rely on checked-in
##      wasm binaries. Requires clang, wasm-ld, a WASI sysroot, and node.
wasm-site:
	$(MAKE) -C $(ROOT) _wasm-site

## wasm-serve: Build the wasm target (full `make wasm`), then static-host
##      $(WASM_SERVE_DIR) at http://localhost:$(WASM_SERVE_PORT)/ and open it in
##      your browser. Long-running dev server — Ctrl-C to stop. Override the port
##      with WASM_SERVE_PORT=<n>. (`make wasm` itself stays headless for CI.)
wasm-serve:
	$(MAKE) -C $(ROOT) _wasm-serve

## docs-html-test: Browser acceptance for the HTML documentation export. Real
##                 Chromium over file:// — the strictest case, and the one a
##                 reader hits first — covering every theme, custom stylesheet
##                 ordering, user guides, search with no server behind it, the
##                 mobile disclosure and keyboard navigation. Reuses the bank
##                 e2e project's Playwright install rather than adding another.
docs-html-test:
	$(MAKE) -C $(ROOT) _docs-html-test
