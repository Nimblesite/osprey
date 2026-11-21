# Each Android app's tests on the selected emulator or device. One target runs one app. Recipes run
# from the repo root through scripts/make/forward.mk. Tests with no host at all: src/toolchain/targets/tests.
ROOT ?= ../../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bank casting casting-headless casting-update inbox tally tally-headless

## bank: Talon Bank on the selected Android device: the host opens it with no person present, requires a
##       first frame and presses its first control. Its rules are tested host-free by `tests bank-app`.
bank:
	$(BANK_ANDROID) bash src/toolchain/hosts/android/run.sh --view-check

## casting: Build and run native casting journeys on the selected emulator/device, recording screenshots.
casting:
	bash src/osprey/examples/casting/android/run.sh
	python3 src/osprey/examples/casting/mobile_test.py android

## casting-headless: Run those native journeys on a headless emulator selected by OSPREY_ANDROID_SERIAL.
casting-headless: casting

## casting-update: Run native casting journeys and rewrite screenshot goldens for visual review.
casting-update:
	OSPREY_UPDATE_GOLDENS=1 $(MAKE) -f src/toolchain/targets/android/tests/Makefile casting

## inbox: The Android goldens, tools and smoke run for the inbox app.
# The goldens need the runtime archives the build makes, so they are ordered by a recipe line rather
# than listed as a sibling prerequisite: under `make -j` siblings may run at once, and the corpus would
# race the archive it links.
inbox: _android-inbox
	$(MAKE) _test_android_goldens
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/mobile/test-android.sh
	OSPREY_ANDROID_SKIP_BUILD=1 bash src/osprey/examples/mobile/android/run.sh --smoke

## tally: Tally on the selected Android device: its view check, then its screen against
##        goldens/android (OSPREY_UPDATE_GOLDENS=1 rewrites them for review).
tally:
	bash src/toolchain/hosts/android/run.sh --view-check
	OSPREY_ANDROID_SKIP_BUILD=1 bash src/toolchain/hosts/android/run.sh --golden

## tally-headless: The same on a headless emulator selected by OSPREY_ANDROID_SERIAL.
tally-headless: tally
endif
