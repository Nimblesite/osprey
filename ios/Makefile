# The iPhone apps. Recipes run from the repo root through scripts/make/forward.mk; `make help` at the
# root lists these too. Each app's tests: ios/tests, and with no host at all, tests.
ROOT ?= ..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bank casting casting-build examples inbox

## bank: Build Talon Bank for iOS devices and the simulator.
bank: _ios-bank

## casting: Build and open the casting player in an iPhone simulator.
casting:
	bash src/osprey/examples/casting/ios/run.sh

## casting-build: Build the unsigned iPhone app; TARGET=ios-sim builds for the simulator.
casting-build:
	bash src/osprey/examples/casting/ios/build.sh $(or $(TARGET),ios)

## examples: Build the iPhone and simulator libraries and SwiftUI example apps.
##           Requires macOS, Xcode and both iOS SDKs. Device output is unsigned.
examples: _ios-examples

## inbox: Build the inbox app for iOS devices and the simulator.
inbox: _ios-inbox
endif
