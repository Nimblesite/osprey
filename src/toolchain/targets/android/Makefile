# The Android apps. Recipes run from the repo root through scripts/make/forward.mk; `make help` at the
# root lists these too. Each app's tests: src/toolchain/targets/android/tests, and with no host at all, src/toolchain/targets/tests.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bank casting casting-build inbox tally

## bank: Build Talon Bank for Android.
bank: _android-bank

## casting: Build and open casting on the Android device named by OSPREY_ANDROID_SERIAL.
casting:
	bash src/osprey/examples/casting/android/run.sh

## casting-build: Build the casting APK: the shared GPUI host, for ARM64 unless a target is named.
casting-build:
	bash src/osprey/examples/casting/android/build.sh

## inbox: Build the inbox app for Android.
inbox: _android-inbox

## tally: Build and open Tally on the Android device named by OSPREY_ANDROID_SERIAL.
tally:
	bash src/toolchain/hosts/android/run.sh
endif
