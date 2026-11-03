ROOT ?= ../..
.PHONY: casting casting-build

## casting: Build and open casting on the Android device named by OSPREY_ANDROID_SERIAL.
casting:
	bash $(ROOT)/examples/casting/android/run.sh

## casting-build: Build an ARM64 APK; TARGET=android-x64 builds an x64 APK.
casting-build:
	bash $(ROOT)/examples/casting/android/build.sh $(or $(TARGET),android-arm64)
