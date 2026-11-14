# Each iPhone app's tests on the simulator. One target runs one app. Recipes run from the repo root
# through scripts/make/forward.mk. Tests with no host at all: src/toolchain/targets/tests.
ROOT ?= ../../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bank casting casting-headless casting-update examples inbox tally tally-headless

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
	OSPREY_UPDATE_GOLDENS=1 $(MAKE) -f src/toolchain/targets/ios/tests/Makefile casting

## examples: Validate C ABI calls, language goldens and the SwiftUI simulator app.
examples: _ios-examples
	$(MAKE) _test_ios

## inbox: Smoke-test the built inbox app on iOS.
inbox: _ios-inbox
	OSPREY_IOS_SKIP_BUILD=1 bash src/osprey/examples/mobile/ios/run.sh --smoke

## tally: Tally's view check on the SwiftUI host in an iPhone simulator, with its window open.
tally:
	open -a Simulator
	$(MAKE) -f src/toolchain/targets/ios/tests/Makefile tally-headless

## tally-headless: Tally's screen against goldens/ios, then its view check with no Simulator window
##                 (OSPREY_UPDATE_GOLDENS=1 rewrites them for review).
# The picture leaves the app running, so the view check also exercises its restart.
tally-headless:
	OSPREY_SWIFT_APP=src/osprey/examples/tally OSPREY_IOS_NAME=Tally OSPREY_IOS_ID=org.ospreylang.tally \
		bash src/toolchain/hosts/swift/ios.sh ios-sim --golden --view-check
endif
