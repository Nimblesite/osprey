# Each web app's tests in a real browser. One target runs one app. Recipes run from the repo root
# through scripts/make/forward.mk. Tests with no host at all: src/toolchain/targets/tests.
ROOT ?= ../../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bank cms

## bank: Browser end-to-end tests for the Talon Bank modules showcase
##       (src/osprey/examples/talon): real Chromium via Playwright drives
##       the compiled osprey binary serving its HTTP API and web UI.
##       Each run picks free ports and gets its own hold marker and database; a hold
##       marker left behind fails the run. Then the journey every host plays
##       (app/self-test.json) runs at desktop and phone width against its goldens.
bank: _bank-web $(BANK_E2E_NODE_DEPS)
	@echo "==> Bank e2e (Playwright)..."
	@run="$$(mktemp -d -t talon-bank-e2e)" && export TALON_RUN_DIR="$$run" \
	  && cd src/osprey/examples/talon/playwright-e2e-tests && npx playwright install chromium \
	  && npx playwright test && npx playwright test -c journey.config.js; status=$$?; \
	  test ! -e "$$run/hold" || { echo "FAIL: bank e2e left its hold marker behind"; status=1; }; \
	  rm -f "$$run/ledger.db" "$$run/ledger.db-journal" "$$run/ledger.db-wal" "$$run/ledger.db-shm"; rmdir "$$run" || status=1; exit $$status

## cms: Browser end-to-end journey for the Git CMS editor (src/osprey/examples/cms): real Chromium
##      drives the Osprey Native editor: tree, selection, syntax colours, blocks, save, publish.
cms: export PATH := $(WASM_PATH_PREFIX)$(PATH)
cms: build _runtime_wasm $(CMS_E2E_NODE_DEPS)
	cd src/osprey/examples/cms/tests && npx playwright install chromium && OSPREY="$(CURDIR)/$(BIN)" npx playwright test
endif
