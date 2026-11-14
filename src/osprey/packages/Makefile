# Package and registry targets run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: package-coverage package-checks package-coverage-record

.PHONY: synth
## synth: Run UI-free synthesis, routing and Signal journeys with source coverage.
synth:
	python3 scripts/packages/package_coverage.py --collect --compiler $(BIN) --package synth --reports $(COVERAGE_DIR)/packages --thresholds $(COVERAGE_THRESHOLDS_FILE)

## package-coverage: per-package source coverage — collect, then gate all 24.
##                   Reruns each src/osprey/packages/<name>/tests suite with `osprey test
##                   --coverage-json` and enforces every threshold under
##                   projects.packages (90% floor, one report per package).
package-coverage: build
	$(MAKE) _test_packages
	$(MAKE) _coverage_check_packages

## package-checks: Package governance, licences, hashing, interruption, graph and
##   workspace UI under GC and ARC. Expect the 19 bundles to match
##   src/osprey/packages/license-audit.json, every selected test to pass, and
##   `Package workspace WASM startup: PASS`. Governance: spec 0045; canonical schema
##   src/osprey/packages/manager/supabase/migrations/schema.yaml. Its Postgres migration is
##   BLOCKED (DataProvider 0.9.11-beta inspects only `public`); keep that failure:
##     dotnet tool restore && dotnet tool run DataProviderMigrate migrate --schema \
##       src/osprey/packages/manager/supabase/migrations/schema.yaml --provider postgres \
##       --output "$$REGISTRY_MIGRATION_CONNECTION"   # passfile, never a password
##   Final manifest/lock writes are separate: never report crash-safe transactions.
package-checks: build
	node scripts/packages/verify-package-licenses.mjs
	node --test scripts/packages/test-package-licenses.mjs
	python3 scripts/packages/test-package-sha256.py
	$(BIN) src/osprey/packages/manager/fixture-check --run
	for filter in "schema-check: model declarations" "schema-check: accepted values" "schema-check: rejected values" \
	  "schema: spec block equals canonical file" "fixtures: names"; do \
	  $(BIN) test src/osprey/packages/manager/tests --filter "$$filter" || exit 1; \
	done
	for filter in "publish: licenses survive publication" "fetch: licenses and attribution are preserved"; do \
	  $(BIN) test src/osprey/packages/manager/cli/tests --filter "$$filter" || exit 1; \
	done
	for memory in gc arc; do \
	  $(BIN) test src/osprey/packages/manager/tests --memory=$$memory --filter "sha256: raw digest" || exit 1; \
	  $(BIN) test src/osprey/packages/manager/cli/tests --memory=$$memory --filter "add: partial" || exit 1; \
	  $(BIN) test src/osprey/packages/manager/cli/tests --memory=$$memory --filter "tree graph: diamond and identical leaf across orgs" || exit 1; \
	  $(BIN) test src/osprey/packages/manager/cli/tests --memory=$$memory --filter "reports: success output is not a finding" || exit 1; \
	  $(BIN) test src/osprey/packages/manager/editor/tests --memory=$$memory || exit 1; \
	  $(BIN) test src/osprey/packages/manager/editor/wire-tests --memory=$$memory || exit 1; \
	done
	$(BIN) src/osprey/packages/vscode/tests --run --quiet
	$(MAKE) target/package-workspace.wasm
	node $(EXT_DIR)/scripts/verify-package-workspace.mjs target/package-workspace.wasm

## package-coverage-record: Write dated measurements to
##   target/coverage/packages/measurements.json; coverage-thresholds.json stays
##   byte-identical. Stale evidence records `null`; a failed gate exits nonzero.
package-coverage-record:
	python3 scripts/packages/package_coverage.py --record-measurements

## fastapi-e2e: Compile the Python flavor FastAPI port under GC and ARC in
##              parallel; verify real HTTP routing, errors, bodies and headers.
.PHONY: fastapi-e2e
fastapi-e2e:
	cd $(ROOT) && python3 src/osprey/packages/fastapi/tests/e2e.cpython

.PHONY: registry-test registry-serve registry-serve-sample registry-blockers

## registry-test: Run the Osprey package-system suites.
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
