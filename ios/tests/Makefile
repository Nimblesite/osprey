# Each iPhone app's tests on the simulator. One target runs one app. Recipes run from the repo root
# through scripts/make/forward.mk. Tests with no host at all: tests.
ROOT ?= ../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bank casting casting-headless casting-update examples inbox

## bank: Talon Bank against its live API on iOS.
bank: _ios-bank _runtime
	OSPREY_IOS_SKIP_BUILD=1 python3 scripts/mobile/bank-mobile-test.py -- bash src/osprey/examples/projects/modules/mobile/ios/test.sh

## casting: Run the casting player's native UI journey with the simulator visible.
casting:
	OSPREY_SIMULATOR_VISIBLE=1 bash src/osprey/examples/casting/ios/test.sh

## casting-headless: Run the same casting UI journey and screenshot capture without opening a viewer.
casting-headless:
	OSPREY_SIMULATOR_VISIBLE=0 bash src/osprey/examples/casting/ios/test.sh

## casting-update: Run native casting journeys and rewrite screenshot goldens for visual review.
casting-update:
	OSPREY_UPDATE_GOLDENS=1 $(MAKE) -f ios/tests/Makefile casting

## examples: Validate C ABI calls, language goldens and the SwiftUI simulator app.
examples: _ios-examples
	$(MAKE) _test_ios

## inbox: Smoke-test the built inbox app on iOS.
inbox: _ios-inbox
	OSPREY_IOS_SKIP_BUILD=1 bash src/osprey/examples/mobile/ios/run.sh --smoke
endif
