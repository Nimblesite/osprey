ROOT ?= ../..
.PHONY: casting casting-build

## casting: Build and open the casting player in an iPhone simulator.
casting:
	bash $(ROOT)/examples/casting/ios/run.sh

## casting-build: Build the unsigned iPhone app; TARGET=ios-sim builds for the simulator.
casting-build:
	bash $(ROOT)/examples/casting/ios/build.sh $(or $(TARGET),ios)
