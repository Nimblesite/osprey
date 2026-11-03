ROOT ?= ../..
.PHONY: casting casting-build

## casting: Build and open casting on the Android device named by OSPREY_ANDROID_SERIAL.
casting:
	bash $(ROOT)/examples/casting/android/run.sh

## casting-build: Build the Android APK with ARM64; TARGET=android-x64 adds the x64 slice.
casting-build:
	bash $(ROOT)/examples/casting/android/build.sh $(or $(TARGET),android-arm64)
