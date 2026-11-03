ROOT ?= ../../..
.PHONY: casting casting-headless

## casting: Run the casting player's native UI journey with the simulator visible.
casting:
	open -a Simulator
	bash $(ROOT)/examples/casting/ios/test.sh

## casting-headless: Run the same casting UI journey and screenshot capture without opening Simulator.
casting-headless:
	bash $(ROOT)/examples/casting/ios/test.sh
