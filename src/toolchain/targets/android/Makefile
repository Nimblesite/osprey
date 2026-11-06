ROOT ?= ../../../..
.PHONY: casting casting-build

## casting: Build and open casting on the Android device named by OSPREY_ANDROID_SERIAL.
casting:
	bash $(ROOT)/src/osprey/examples/casting/android/run.sh

## casting-build: Build the casting APK: the shared GPUI host, for ARM64 unless a target is named.
casting-build:
	bash $(ROOT)/src/osprey/examples/casting/android/build.sh
