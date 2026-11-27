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

## package-checks: Package governance, licences, hashing, interruption, graph and
##   workspace UI under GC and ARC. Expect the 19 bundles to match
##   src/osprey/packages/license-audit.json, every selected test to pass, and
##   `Package workspace WASM startup: PASS`. Governance: spec 0045; canonical schema
##   src/osprey/packages/manager/supabase/migrations/schema.yaml. Its Postgres migration is
##   verified with the pinned registry-migration-tool against a private registry schema.
##   Final manifest/lock writes are separate: never report crash-safe transactions.
package-checks: build
	node scripts/packages/verify-package-licenses.mjs
	node --test scripts/packages/test-package-licenses.mjs
	python3 scripts/packages/test-package-sha256.py
	$(BIN) src/osprey/packages/manager/fixture-check --run
	for filter in "schema-check: model declarations" "schema-check: accepted values" "schema-check: rejected values" \
	  "schema: spec references canonical model" "fixtures: names"; do \
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

.PHONY: registry-test registry-serve registry-serve-sample registry-blockers

REGISTRY_WEB = src/osprey/packages/manager/web
REGISTRY_TERRAFORM = src/osprey/packages/manager/ops/terraform
REGISTRY_MIGRATOR_REV = 3b4e356522affc61b466d8e87ef5232319c9d7a0
REGISTRY_MIGRATOR_SHA256 = 87192312ca3a313fa0c786705c2fa2cbafebe76a0afb522b39df82f08aaa740a
REGISTRY_MIGRATOR_DIR = target/registry-tools/$(REGISTRY_MIGRATOR_REV)
REGISTRY_MIGRATOR = $(REGISTRY_MIGRATOR_DIR)/bin/DataProviderMigrate.dll

.PHONY: registry-migration-tool registry-migration-check
## registry-migration-tool: Build the pinned private-schema DataProvider CLI under target/registry-tools.
registry-migration-tool: $(REGISTRY_MIGRATOR)
	@echo "$(abspath $(REGISTRY_MIGRATOR))"

$(REGISTRY_MIGRATOR):
	@mkdir -p $(REGISTRY_MIGRATOR_DIR)
	@set -eu; scratch=$$(mktemp -d "$(REGISTRY_MIGRATOR_DIR)/build.XXXXXX"); \
	  trap 'rm -rf "$$scratch"' EXIT; \
	  curl -fsSL --retry 2 --max-time 120 "https://codeload.github.com/Nimblesite/DataProvider/tar.gz/$(REGISTRY_MIGRATOR_REV)" -o "$$scratch/source.tgz"; \
	  echo "$(REGISTRY_MIGRATOR_SHA256)  $$scratch/source.tgz" | shasum -a 256 -c -; \
	  tar -xzf "$$scratch/source.tgz" --strip-components=1 -C "$$scratch"; \
	  dotnet publish "$$scratch/Migration/DataProviderMigrate" -c Release -f net10.0 -o "$(abspath $(REGISTRY_MIGRATOR_DIR))/bin"

## registry-migration-check: Apply the ownership schema twice on disposable socket-only PostgreSQL.
registry-migration-check: $(REGISTRY_MIGRATOR) $(BIN)
	@set -eu; pg=$$(pg_config --bindir); \
	  if [ ! -x "$$pg/postgres" ]; then pg=$$(dirname "$$(realpath "$$(command -v postgres)")"); fi; \
	  db=$$(mktemp -d /tmp/osprey-registry.XXXXXX); \
	  cleanup() { if [ -f "$$db/data/postmaster.pid" ]; then "$$pg/pg_ctl" -D "$$db/data" -m fast -w stop; fi; rm -rf "$$db"; }; \
	  trap cleanup EXIT; export PGHOST="$$db" PGPORT=18873 PGUSER=registry_check PGDATABASE=postgres; \
	  "$$pg/initdb" -D "$$db/data" -U "$$PGUSER" --auth-local=trust --auth-host=reject --no-locale >/dev/null; \
	  "$$pg/pg_ctl" -D "$$db/data" -l "$$db/server.log" -o "-k $$db -p $$PGPORT -c listen_addresses=''" -w start; \
	  "$$pg/psql" -X -v ON_ERROR_STOP=1 -c 'CREATE SCHEMA registry'; \
	  migrate() { dotnet "$(REGISTRY_MIGRATOR)" migrate --provider postgres --schema src/osprey/packages/manager/supabase/migrations/schema.yaml --output "Host=$$PGHOST;Port=$$PGPORT;Username=$$PGUSER;Database=$$PGDATABASE"; }; \
	  migrate; migrate >"$$db/replay.log" || { cat "$$db/replay.log"; exit 1; }; cat "$$db/replay.log"; \
	  grep -Fq 'No operations to apply' "$$db/replay.log"; \
	  test "$$("$$pg/psql" -XAtc "SELECT count(*) = 13 AND bool_and(relrowsecurity AND relforcerowsecurity) FROM pg_class WHERE relnamespace = 'registry'::regnamespace AND relkind = 'r'")" = t; \
	  $(BIN) test src/osprey/packages/manager/database-tests/live

.PHONY: registry-integration-check
## registry-integration-check: Browse a real PostgreSQL-backed hosted API with the packaged website in Chromium.
registry-integration-check: registry-website-fixture
	REGISTRY_BROWSER_CHECK=1 $(MAKE) -C src/osprey/packages registry-migration-check

.PHONY: registry-infra-check
## registry-infra-check: Validate pinned registry infrastructure without contacting a state backend.
registry-infra-check: export TF_DATA_DIR = $(CURDIR)/target/registry-infra/data
registry-infra-check:
	terraform -chdir=$(REGISTRY_TERRAFORM) fmt -check
	terraform -chdir=$(REGISTRY_TERRAFORM) init -backend=false -lockfile=readonly -input=false
	terraform -chdir=$(REGISTRY_TERRAFORM) validate

.PHONY: registry-hosted-api
## registry-hosted-api: Compile the native Cloud Run API listener for the current platform.
registry-hosted-api: target/osprey-registry-hosted

target/osprey-registry-hosted: $(BIN) $(REGISTRY_CLIENT_SOURCES)
	$(BIN) build src/osprey/packages/manager/api/hosted --quiet --no-warnings -o $@

.PHONY: registry-website registry-website-fixture registry-website-check
## registry-website-check: Build an isolated Pages artifact and exercise its publisher flow in Chromium.
registry-website-check: registry-website-fixture
	node scripts/packages/test-registry-preview.cjs --built-hosted-account

registry-website-fixture: export REGISTRY_API_ORIGIN := https://api.example.test
registry-website-fixture: export REGISTRY_CONTENT_ORIGIN := https://content.example.test

## registry-website: Compile the registry website to WASM and render its Cloudflare Pages directory.
registry-website registry-website-fixture registry-website-archive: $(BIN) | _runtime_wasm
	@mkdir -p target/registry-site-build
	$(BIN) build $(REGISTRY_WEB)/site --target=wasm32 --quiet --no-warnings -o target/registry-site-build/site.wasm
	@set -eu; out=$$(mktemp -d target/registry-site-build/pages.XXXXXX); \
	  trap 'rm -rf "$$out"' EXIT; \
	  render() { printf '%s\n' "$$1" | wasmtime target/registry-site-build/site.wasm; }; \
	  render missing > "$$out/404.html"; \
	  for page in guide publishing; do \
	    mkdir "$$out/$$page"; render "$$page" > "$$out/$$page/index.html"; \
	  done; \
	  mkdir "$$out/upload"; origin="$${REGISTRY_API_ORIGIN:-}"; content="$${REGISTRY_CONTENT_ORIGIN:-}"; \
	  if [ "$@" = registry-website-archive ] && { [ -z "$$origin" ] || [ -z "$$content" ]; }; then echo "Set REGISTRY_API_ORIGIN and REGISTRY_CONTENT_ORIGIN." >&2; exit 1; fi; \
	  node -e 'const [api, content] = process.argv.slice(1); if (content && (!api || api === content)) process.exit(1); for (const origin of [api, content].filter(Boolean)) { const u = new URL(origin); if (u.protocol !== "https:" || u.origin !== origin || !/^[a-z0-9.-]+$$/.test(u.hostname)) process.exit(1); }' "$$origin" "$$content"; \
	  script=src/toolchain/hosts/web/src/registry-form.mjs; adapter="form.$$(shasum -a 256 "$$script" | cut -c1-12).mjs"; \
	  cp "$$script" "$$out/$$adapter"; \
	  asset() { file="$$1"; name="$$2.$$(shasum -a 256 "$$file" | cut -c1-12).$$3"; cp "$$file" "$$out/$$name"; echo "$$name"; }; \
	  catalog=$$(asset src/toolchain/hosts/web/src/registry-catalog.mjs catalog mjs); host=$$(asset src/osprey/examples/wasm/wasi-shim.mjs wasi mjs); guest=$$(asset target/registry-site-build/site.wasm site wasm); \
	  printf "%s\n" home "$$origin" "$$catalog" "$$host" "$$guest" "$$content" | wasmtime target/registry-site-build/site.wasm > "$$out/index.html"; \
	  printf "%s\n" hosted-package "$$origin" "$$catalog" "$$host" "$$guest" "$$content" | wasmtime target/registry-site-build/site.wasm > "$$out/package.html"; \
	  printf '/packages/* /package.html 200\n/publishers/* /package.html 200\n/tags/* /package.html 200\n' > "$$out/_redirects"; \
	  printf "%s\n" upload "$$origin" "$$adapter" | wasmtime target/registry-site-build/site.wasm > "$$out/upload/index.html"; \
	  digest=$$(shasum -a 256 $(REGISTRY_WEB)/assets/registry.css | cut -c1-12); \
	  asset="registry.$$digest.css"; grep -Fq "href=\"/$$asset\"" "$$out/index.html"; \
	  cp $(REGISTRY_WEB)/assets/registry.css "$$out/$$asset"; \
	  cp $(REGISTRY_WEB)/assets/registry-mark.svg "$$out/"; \
	  sed "s@form-action@script-src 'self' 'wasm-unsafe-eval'; connect-src 'self' $$origin; form-action@;s@form-action 'none'@form-action 'self'@" $(REGISTRY_WEB)/assets/_headers > "$$out/_headers"; \
	  if [ "$@" = registry-website-archive ]; then python3 scripts/packages/archive-registry-site.py "$$out" target/registry-artifacts; \
	  else rsync -a --delete "$$out/" $(if $(filter registry-website-fixture,$@),target/registry-site-build/hosted-fixture,target/registry-site)/; fi

.PHONY: registry-website-archive
## registry-website-archive: Build and archive the configured Pages site by SHA-256 for deployment.
registry-website-archive:

.PHONY: registry-website-serve
## registry-website-serve: Serve the built preview and live folder catalog together on localhost:4187.
registry-website-serve:
	python3 scripts/packages/serve-registry-preview.py

.PHONY: registry-website-preview
## registry-website-preview: Render searchable local packages from registry-serve into target/registry-site-preview.
registry-website-preview: registry-website target/osprey-pkg
	bash scripts/packages/registry-preview.sh

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
