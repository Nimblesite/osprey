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
## synth: Run UI-free synthesis, routing and Signal journeys with source coverage, then compile every
##        project built on synth (the daw, its test and render projects, the pocket orchestra kernel).
synth:
	python3 scripts/packages/package_coverage.py --collect --compiler $(BIN) --package synth --reports $(COVERAGE_DIR)/packages --thresholds $(COVERAGE_THRESHOLDS_FILE)
	node scripts/make/package-consumers.mjs $(BIN) synth

## package-coverage: per-package source coverage — collect, then gate all 24.
##                   Reruns each src/osprey/packages/<name>/tests suite with `osprey test
##                   --coverage-json` and enforces every threshold under
##                   projects.packages (90% floor, one report per package).
package-coverage: build
	$(MAKE) _test_packages
	$(MAKE) _coverage_check_packages

## package-checks: Package licences, hashing, the VS Code package suite and the package workspace
##   WASM. Expect the 19 bundles to match src/osprey/packages/license-audit.json, every selected
##   test to pass, and `Package workspace WASM startup: PASS`. The workspace is built from
##   Nimblesite/osprey-packages, so run `make downstream` first; the governance, schema and
##   migration suites run in that repository. [REPO-SPLIT]
package-checks: build
	node scripts/packages/verify-package-licenses.mjs
	node --test scripts/packages/test-package-licenses.mjs
	python3 scripts/packages/test-package-sha256.py
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

## express-e2e: Compile the Express examples under GC and ARC, serve them
##              natively and compare every reply over real HTTP with what the
##              pinned real Express replied to the same files. Expected result:
##              "52 replies matched, 0 failed".
.PHONY: express-e2e
express-e2e:
	cd $(ROOT) && node src/osprey/packages/express/tests/e2e.mjs

## express-bench: Serve the Express examples from Osprey (GC and ARC) and from
## real Express on Node.js, time the same requests against each over loopback,
## and print a table. Results: target/express/benchmark/results.json.
.PHONY: express-bench
express-bench:
	cd $(ROOT) && node src/osprey/packages/express/oracle/bench.mjs

## express-oracle: Re-record tests/examples.expected.json from the pinned real
##                 Express on Node.js (development only; CI compares with the
##                 committed recording).
.PHONY: express-oracle
express-oracle:
	cd $(ROOT)/src/osprey/packages/express/oracle && npm ci --no-audit --no-fund && node record.mjs

.PHONY: registry-serve
## registry-serve: Serve this repository's packages (src/osprey/packages/*, published
##                 into target/local-registry) over HTTP on 127.0.0.1:8787
##                 with the Osprey folder-registry server. Ctrl-C stops it.
registry-serve: target/osprey-registry target/local-registry.stamp
	@$(MKDIR) target/local-registry-server
	@printf '{ "root": "%s", "address": "127.0.0.1", "port": 8787 }\n' "$(abspath target/local-registry)" \
	  > target/local-registry-server/registry-server.json
	cd target/local-registry-server && $(abspath target/osprey-registry)

endif
