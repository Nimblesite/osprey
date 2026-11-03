ROOT ?= ../../..
.PHONY: casting casting-headless

## casting: Build and run native casting journeys on the selected emulator/device, recording screenshots.
casting:
	bash $(ROOT)/examples/casting/android/run.sh
	python3 $(ROOT)/examples/casting/android/test.py

## casting-headless: Run those native journeys on a headless emulator selected by OSPREY_ANDROID_SERIAL.
casting-headless: casting
