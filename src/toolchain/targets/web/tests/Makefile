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
##       (src/osprey/examples/projects/modules): real Chromium via Playwright drives
##       the compiled osprey binary serving its HTTP API and web UI.
##       The server's hold marker must not outlive the run: a leaked one
##       makes every later bank test refuse to start.
bank: _bank-web $(BANK_E2E_NODE_DEPS)
	@echo "==> Bank e2e (Playwright)..."
	cd src/osprey/examples/projects/modules/e2e && npx playwright install chromium && npx playwright test
	@test ! -e /tmp/talon_bank.hold || { echo "FAIL: bank e2e left /tmp/talon_bank.hold behind; its webServer was stopped without running its trap"; exit 1; }
endif
