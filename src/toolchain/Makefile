# The Rust workspace's own gates. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

## hawk: Dead-code gate (astral-sh/hawk). Fails the build when any `pub`
## declaration is unreachable from the osprey binary (hawk::dead_public).
##
## hawk counts the workspace's TEST binaries as reachability roots, so an item
## whose only callers are its own `#[cfg(test)]` module passes this gate — that
## is how `osprey_debug::DebugBuild` and `osprey_syntax::dependency_sets` lived
## in the tree. `scripts/ci/verify-no-dead-code.mjs` (run by `_lint`) answers the
## narrower question "does PRODUCT code name this?" and catches that class.
## Neither gate subsumes the other; both run. Scoped
## to dead_public ONLY — unnecessary_public / restricted-visibility findings are
## over-exposure, not dead code, and several are irreducibly public for the
## integration tests under this workspace's `dead_code = "deny"` policy, so they
## must NOT fail the gate. hawk needs rustc_private (RUSTC_BOOTSTRAP=1) and its
## prebuilt driver is built against one exact rustc and refuses any other, so
## the gate runs hawk under that toolchain, not under whatever stable is today:
## CI installing the latest hawk over the latest stable failed the day stable
## moved to 1.99.1 while hawk 0.1.15 was built for 1.99.0. HAWK_VERSION and
## HAWK_RUSTC move together; `make -C src/toolchain hawk-install` installs both. An absent
## cargo-hawk FAILS this
## target: a gate that cannot run must not report success.
## Install: make -C src/toolchain hawk-install (https://github.com/astral-sh/hawk)
HAWK_VERSION ?= 0.1.15
HAWK_RUSTC   ?= 1.99.0
.PHONY: hawk hawk-install
hawk-install:
	rustup toolchain install $(HAWK_RUSTC) --profile minimal
	curl --proto '=https' --tlsv1.2 -LsSf https://github.com/astral-sh/hawk/releases/download/$(HAWK_VERSION)/cargo-hawk-installer.sh | sh

hawk:
	@echo "==> Dead-code gate (hawk)..."
	@if ! command -v cargo-hawk >/dev/null 2>&1; then \
		echo "FAIL: cargo-hawk is not installed, so the dead-code gate cannot run."; \
		echo "      A gate that cannot run must not report success — this used to"; \
		echo "      print a warning and exit 0, so 'make -C src/toolchain hawk' was green on every"; \
		echo "      machine without the binary, the CI runner included if its"; \
		echo "      install step ever stopped landing cargo-hawk on PATH."; \
		echo "      Install: make -C src/toolchain hawk-install"; \
		exit 1; \
	fi
	cd src/toolchain && RUSTC_BOOTSTRAP=1 cargo +$(HAWK_RUSTC) hawk check --only dead-public -D hawk::dead_public --target-dir $(CURDIR)/target/hawk
endif
