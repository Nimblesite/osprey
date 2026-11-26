# The web apps: Osprey compiled to WebAssembly and served to the browser. Recipes run from the repo root
# through scripts/make/forward.mk; `make help` at the root lists these too. Their tests: src/toolchain/targets/web/tests.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bank bank-build cms

## bank: Rebuild and run the Talon Bank showcase for manual testing: two processes, the web client
##       (static files on http://127.0.0.1:18791) and the server it calls (JSON API on :18790).
##       The hold marker keeps the server up; Ctrl-C removes it and stops both.
bank: _bank-web
	@echo "==> Talon Bank: web client http://127.0.0.1:18791, API http://127.0.0.1:18790  (Ctrl-C to stop)"
	@touch /tmp/talon_bank.hold
	@node src/toolchain/hosts/web/scripts/serve.mjs --root src/toolchain/hosts/web/build/talon --port 18791 & web=$$!; \
	  trap 'rm -f /tmp/talon_bank.hold; kill $$web 2>/dev/null' EXIT INT TERM; \
	  (if command -v open >/dev/null && command -v curl >/dev/null; then \
	    attempts=0; \
	    until accounts="$$(curl -fsS http://127.0.0.1:18790/api/accounts 2>/dev/null)" && [[ "$$accounts" == *'"Priya Sharma"'* ]]; do \
	      attempts=$$((attempts + 1)); [ "$$attempts" -ge 200 ] && exit 0; sleep 0.1; \
	    done; \
	    open http://127.0.0.1:18791; \
	  fi) & \
	  ./$(BIN) src/osprey/examples/talon/backend --run

## cms: Build and run the Git CMS (src/osprey/examples/cms) on fresh throwaway sites, each with a bare
##      "published" remote: the fixture at http://127.0.0.1:4397 and a copy of the Eleventy sample
##      site at http://127.0.0.1:4398 (Ctrl-C to stop).
cms: export PATH := $(WASM_PATH_PREFIX)$(PATH)
cms: build _runtime_wasm $(CMS_E2E_NODE_DEPS)
	@(sleep 20; command -v open >/dev/null && open http://127.0.0.1:4397) &
	OSPREY="$(CURDIR)/$(BIN)" node src/osprey/examples/cms/tests/serve.mjs

## bank-build: Rebuild Talon Bank's web client (static files in src/toolchain/hosts/web/build/talon).
##             Requires Node and a WASI sysroot.
bank-build: _bank-web
endif
