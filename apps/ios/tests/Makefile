ROOT ?= ../../..
.PHONY: casting casting-headless casting-update

## casting: Run the casting player's native UI journey with the simulator visible.
casting:
	OSPREY_SIMULATOR_VISIBLE=1 bash $(ROOT)/examples/casting/ios/test.sh

## casting-headless: Run the same casting UI journey and screenshot capture without opening a viewer.
casting-headless:
	OSPREY_SIMULATOR_VISIBLE=0 bash $(ROOT)/examples/casting/ios/test.sh

## casting-update: Run native casting journeys and rewrite screenshot goldens for visual review.
casting-update:
	OSPREY_UPDATE_GOLDENS=1 $(MAKE) casting
