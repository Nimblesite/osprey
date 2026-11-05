# Each app's headless tests: no host, no window and no network, so they run the same on every
# platform. One target runs one app. Tests on a real host: src/toolchain/targets/macos/tests and src/toolchain/targets/windows/tests.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: agent bank bank-mobile casting flock inbox painting signal-flow storage synth workbench

## agent: Osprey Agent's turn logic and its real update path, with scripted completions.
agent: _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(MAKE) _test_agent

## bank: Talon Bank's pure domain layer.
bank: build
	$(MAKE) _bank_test

## bank-mobile: The Talon Bank mobile app's shared domain.
bank-mobile: _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(BIN) src/osprey/examples/projects/modules/mobile/app/test --run

## casting: The casting player, driven by accessible label. [UI-MACHINE-TEST]
casting:
	$(MAKE) -C $(ROOT) _app-suite SUITES=src/osprey/examples/casting/test

## casting-mobile-build: Cross-compile the casting C ABI; TARGET selects ios, ios-sim, android-arm64 or android-x64.
.PHONY: casting-mobile-build
casting-mobile-build:
	bash $(ROOT)/src/osprey/examples/casting/mobile-build.sh $(or $(TARGET),ios-sim)

## casting-mobile-tools: The unfinished mobile apps' tool and bridge regressions, which CI does not run.
.PHONY: casting-mobile-tools
casting-mobile-tools:
	python3 $(ROOT)/src/osprey/examples/casting/test_mobile_tools.py
	cargo test --manifest-path $(ROOT)/src/toolchain/hosts/gpui/Cargo.toml --features mobile-bridge-tests adapters::mobile

## casting-mobile-transport: Exercise the native Cast TLS transport against a real loopback receiver.
.PHONY: casting-mobile-transport
casting-mobile-transport:
	bash $(ROOT)/src/osprey/examples/casting/mobile-transport-test.sh $(or $(PLATFORM),ios)

## flock: Flock's herding game logic.
flock:
	$(MAKE) -C $(ROOT) _app-suite SUITES=src/osprey/examples/gpui/flock/test

## inbox: The shared mobile inbox domain and its build tools.
inbox: _runtime
	python3 scripts/mobile/test-mobile-tools.py
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(BIN) src/osprey/examples/mobile/inbox/test --run

## painting: The painting port's stroke logic.
painting:
	$(MAKE) -C $(ROOT) _app-suite SUITES=src/osprey/examples/gpui/painting/test

## signal-flow: The signal-flow editor, driven by accessible label. [UI-MACHINE-TEST]
signal-flow:
	$(MAKE) -C $(ROOT) _app-suite SUITES=src/osprey/packages/signal_flow/tests

## storage: Osprey Storage's domain.
storage:
	$(MAKE) -C $(ROOT) _app-suite SUITES=src/osprey/examples/storage/test

## synth: The Prism synth, driven by accessible label. [UI-MACHINE-TEST]
synth:
	$(MAKE) -C $(ROOT) _app-suite SUITES=src/osprey/examples/synth/test

## workbench: The Proof Workbench's imported-mathematics cases under GC and ARC.
workbench: build
	for memory in gc arc; do \
	  $(BIN) test src/osprey/workbench/tests --memory=$$memory --filter "proof library file import displays its actual checked statement" || exit 1; \
	  $(BIN) test src/osprey/workbench/tests --memory=$$memory --filter "imported id_imp exposes its actual statement and no assumptions" || exit 1; \
	done
endif
