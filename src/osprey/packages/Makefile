# Packages and the registry. Each target runs its recipe in the root Makefile, where it is the
# private `_` target of the same name; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..

.PHONY: package-coverage package-checks package-coverage-record registry-test registry-serve registry-serve-sample registry-blockers

## package-coverage: per-package source coverage — collect, then gate all 24.
##                   Reruns each src/osprey/packages/<name>/tests suite with `osprey test
##                   --coverage-json` and enforces every threshold under
##                   projects.packages (90% floor, one report per package).
package-coverage:
	$(MAKE) -C $(ROOT) _package-coverage

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
package-checks:
	$(MAKE) -C $(ROOT) _package-checks

## package-coverage-record: Write dated measurements to
##   target/coverage/packages/measurements.json; coverage-thresholds.json stays
##   byte-identical. Stale evidence records `null`; a failed gate exits nonzero.
package-coverage-record:
	$(MAKE) -C $(ROOT) _package-coverage-record

## registry-test: Osprey package-system suites (src/osprey/packages/manager/tests) under the
##                src/osprey/packages/manager/core arithmetic policy. Excluded from `test` and
##                CI until the package system stabilises (maintainer decision).
registry-test:
	$(MAKE) -C $(ROOT) _registry-test

## registry-serve: Serve this repository's packages (src/osprey/packages/*, published
##                 into target/local-registry) over HTTP on 127.0.0.1:8787
##                 with the Osprey folder-registry server. Ctrl-C stops it.
registry-serve:
	$(MAKE) -C $(ROOT) _registry-serve

## registry-serve-sample: Serve src/osprey/packages/manager/sample-registry (the @demo
##                        packages the e2e consumer uses) on 127.0.0.1:8787
##                        (settings in src/osprey/packages/manager/api/registry-server.json).
registry-serve-sample:
	$(MAKE) -C $(ROOT) _registry-serve-sample

## registry-blockers: Probe the host capabilities plan 0020 still needs
##                    (argv, exit status, binary-safe reads, ...). Expected to
##                    FAIL until each capability lands; never part of `test`.
registry-blockers:
	$(MAKE) -C $(ROOT) _registry-blockers

## fastapi-e2e: Compile the Python flavor FastAPI port under GC and ARC in
##              parallel; verify real HTTP routing, errors, bodies and headers.
.PHONY: fastapi-e2e
fastapi-e2e:
	cd $(ROOT) && python3 src/osprey/packages/fastapi/tests/e2e.py
