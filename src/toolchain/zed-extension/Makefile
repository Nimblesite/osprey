ROOT := $(abspath ../../..)
MANIFEST := $(ROOT)/src/toolchain/Cargo.toml

.PHONY: help prepare build lint test stdio-test

# help: Show the Zed extension's build and verification commands.
help:
	@sed -n 's/^# /  /p' Makefile

# prepare: Synchronize Default highlighting from the canonical grammar queries.
prepare:
	cp ../tree-sitter-osprey/queries/highlights.scm languages/osprey/highlights.scm

# build: Build the Zed WASM extension; requires rustup target add wasm32-wasip2.
build: prepare
	cargo build --manifest-path $(MANIFEST) -p osprey-zed --target wasm32-wasip2

# lint: Check formatting and launch-policy static analysis with warnings as errors.
lint:
	cargo fmt --manifest-path $(MANIFEST) -p osprey-zed -- --check
	cargo clippy --manifest-path $(MANIFEST) -p osprey-zed --all-targets -- -D warnings

# test: Run isolated launch-policy tests (no editor or compiler rebuild required).
test:
	cargo test --manifest-path $(MANIFEST) -p osprey-zed

# stdio-test: Exercise the built compiler's real transport with Zed language IDs.
stdio-test: prepare
	python3 test_stdio.py
