# Packages; the registry targets are in src/osprey/packages/manager. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: package-coverage package-checks package-coverage-record

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


## fastapi-benchmark: Compare the asserted explicit-response HTTP subset against
##                    pinned CPython FastAPI. GC/ARC, fresh connections, five trials;
##                    results in target/fastapi/benchmark/results.json. Informational.
.PHONY: fastapi-benchmark
fastapi-benchmark:
	cd $(ROOT) && python3 src/osprey/packages/fastapi/tests/benchmark.cpython
endif
