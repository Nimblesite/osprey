# Each web app's tests in a real browser. One target runs one app. Recipes run from the repo root
# through scripts/make/forward.mk. Tests with no host at all: src/toolchain/targets/tests.
ROOT ?= ../../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bank

## bank: Browser end-to-end tests for the Talon Bank modules showcase
##       (src/osprey/examples/talon): real Chromium via Playwright drives
##       the compiled osprey binary serving its HTTP API and web UI.
##       Each run picks free ports and its own hold marker and database. Then the journey every host plays
##       (app/self-test.json) runs at desktop and phone width against its goldens.
bank: _bank-web $(BANK_E2E_NODE_DEPS)
	@echo "==> Bank e2e (Playwright)..."
	cd src/osprey/examples/talon/playwright-e2e-tests && npx playwright install chromium && npx playwright test && npx playwright test -c journey.config.js
endif
