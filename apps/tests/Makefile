# Each app's headless tests: no host, no window and no network, so they run the same on every
# platform. One target runs one app. Tests on a real host: apps/macos/tests and apps/windows/tests.
ROOT ?= ../..

.PHONY: agent bank bank-mobile casting flock inbox painting signal-flow storage synth workbench

## agent: Osprey Agent's turn logic and its real update path, with scripted completions.
agent:
	$(MAKE) -C $(ROOT) _agent-suites

## bank: Talon Bank's pure domain layer.
bank:
	$(MAKE) -C $(ROOT) _bank-test

## bank-mobile: The Talon Bank mobile app's shared domain.
bank-mobile:
	$(MAKE) -C $(ROOT) _bank-mobile-domain-test

## casting: The casting player, driven by accessible label. [UI-MACHINE-TEST]
casting:
	$(MAKE) -C $(ROOT) _app-suite SUITES=examples/casting/test

## casting-mobile-build: Cross-compile the casting C ABI; TARGET selects ios, ios-sim, android-arm64 or android-x64.
.PHONY: casting-mobile-build
casting-mobile-build:
	bash $(ROOT)/examples/casting/mobile-build.sh $(or $(TARGET),ios-sim)

## casting-mobile-tools: The unfinished mobile apps' tool and bridge regressions, which CI does not run.
.PHONY: casting-mobile-tools
casting-mobile-tools:
	python3 $(ROOT)/examples/casting/test_mobile_tools.py
	cargo test --manifest-path $(ROOT)/hosts/gpui/Cargo.toml --features mobile-bridge-tests adapters::mobile

## casting-mobile-transport: Exercise the native Cast TLS transport against a real loopback receiver.
.PHONY: casting-mobile-transport
casting-mobile-transport:
	bash $(ROOT)/examples/casting/mobile-transport-test.sh $(or $(PLATFORM),ios)

## flock: Flock's herding game logic.
flock:
	$(MAKE) -C $(ROOT) _app-suite SUITES=examples/gpui/flock/test

## inbox: The shared mobile inbox domain and its build tools.
inbox:
	$(MAKE) -C $(ROOT) _mobile-domain-test

## painting: The painting port's stroke logic.
painting:
	$(MAKE) -C $(ROOT) _app-suite SUITES=examples/gpui/painting/test

## signal-flow: The signal-flow editor, driven by accessible label. [UI-MACHINE-TEST]
signal-flow:
	$(MAKE) -C $(ROOT) _app-suite SUITES=packages/signal_flow/tests

## storage: Osprey Storage's domain.
storage:
	$(MAKE) -C $(ROOT) _app-suite SUITES=examples/storage/test

## synth: The Prism synth, driven by accessible label. [UI-MACHINE-TEST]
synth:
	$(MAKE) -C $(ROOT) _app-suite SUITES=examples/synth/test

## workbench: The Proof Workbench's imported-mathematics cases under GC and ARC.
workbench:
	$(MAKE) -C $(ROOT) _workbench-test
