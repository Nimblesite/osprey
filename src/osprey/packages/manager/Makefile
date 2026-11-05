# The package registry. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: registry-test registry-serve registry-serve-sample registry-blockers

## registry-test: Osprey package-system suites (src/osprey/packages/manager/tests) under the
##                src/osprey/packages/manager/core arithmetic policy. Excluded from `test` and
##                CI until the package system stabilises (maintainer decision).
registry-test: build
	$(MAKE) _test_registry

## registry-serve: Serve this repository's packages (src/osprey/packages/*, published
##                 into target/local-registry) over HTTP on 127.0.0.1:8787
##                 with the Osprey folder-registry server. Ctrl-C stops it.
registry-serve: target/osprey-registry target/local-registry.stamp
	@$(MKDIR) target/local-registry-server
	@printf '{ "root": "%s", "address": "127.0.0.1", "port": 8787 }\n' "$(abspath target/local-registry)" \
	  > target/local-registry-server/registry-server.json
	cd target/local-registry-server && $(abspath target/osprey-registry)

## registry-serve-sample: Serve src/osprey/packages/manager/sample-registry (the @demo
##                        packages the e2e consumer uses) on 127.0.0.1:8787
##                        (settings in src/osprey/packages/manager/api/registry-server.json).
registry-serve-sample: build
	cd src/osprey/packages/manager/api && $(abspath $(BIN)) . --run

## registry-blockers: Probe the host capabilities plan 0020 still needs
##                    (argv, exit status, binary-safe reads, ...). Expected to
##                    FAIL until each capability lands; never part of `test`.
registry-blockers: build
	$(MAKE) _test_registry_blockers
endif
