ROOT ?= ../..
.PHONY: casting casting-build

## casting: Build and open casting on the Android device named by OSPREY_ANDROID_SERIAL.
casting:
	bash $(ROOT)/examples/casting/android/run.sh

## casting-build: Build a universal APK with freshly linked ARM64 and x64 libraries.
casting-build:
	bash $(ROOT)/examples/casting/android/build.sh
