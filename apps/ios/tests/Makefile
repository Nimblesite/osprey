ROOT ?= ../../..
.PHONY: casting casting-headless casting-update

## casting: Run the casting player's native UI journey with the simulator visible.
casting:
	open -a "$$(xcode-select -p)/Applications/Simulator.app"
	bash $(ROOT)/examples/casting/ios/test.sh

## casting-headless: Run the same casting UI journey and screenshot capture without opening Simulator.
casting-headless:
	bash $(ROOT)/examples/casting/ios/test.sh

## casting-update: Run native casting journeys and rewrite screenshot goldens for visual review.
casting-update:
	OSPREY_UPDATE_GOLDENS=1 $(MAKE) casting
