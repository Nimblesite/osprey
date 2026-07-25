#!/bin/bash

# Generate reference documentation from the Osprey compiler (`osprey --docs`).
#
# src/docs/functions/ and src/docs/stdlib.md are GENERATED build output — they
# are gitignored, never committed. When a Rust compiler binary
# (target/release/osprey) with --docs support is present, this script
# regenerates them; otherwise it no-ops and exits successfully, so a
# compiler-free `npm run build` still succeeds (just without the per-function
# reference pages). CI and the GitHub Pages deploy both build the compiler
# before `npm run build`, so the live site always ships freshly generated docs.
# (types/, operators/ and keywords/ are hand-maintained — the current compiler
# does not emit them — so they remain tracked in src/docs/.)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEBSITE_DIR="$(dirname "$SCRIPT_DIR")"
REPO_DIR="$(dirname "$WEBSITE_DIR")"
OSPREY_BIN="$REPO_DIR/target/release/osprey"
DOCS_DIR="$WEBSITE_DIR/src/docs"

echo "Generating Osprey reference documentation..."
mkdir -p "$DOCS_DIR"

if [ ! -x "$OSPREY_BIN" ]; then
    echo "NOTE: $OSPREY_BIN not found (build with: cargo build --release)."
    echo "Using committed docs in $DOCS_DIR"
    exit 0
fi

# Older compilers predate the --docs flag; detect support at runtime so this
# script degrades gracefully (leaves any existing docs in place) instead of
# failing the build.
if ! "$OSPREY_BIN" --help 2>&1 | grep -q -- '--docs'; then
    echo "NOTE: osprey does not support --docs yet; using committed docs in $DOCS_DIR"
    exit 0
fi

echo "Generating API reference from compiler..."
"$OSPREY_BIN" --docs --docs-dir "$DOCS_DIR"

if [ ! -f "$DOCS_DIR/index.md" ]; then
    echo "Error: Documentation generation failed - no docs generated to $DOCS_DIR"
    exit 1
fi

# Create the main API reference page from the generated content.
cat > "$DOCS_DIR/stdlib.md" << 'EOF'
---
layout: page
title: "API Reference - Osprey Programming Language"
description: "Complete API reference for built-in functions, types, operators, and language constructs"
---

# Osprey API Reference

Browse the generated reference for built-in functions, types, operators, and language constructs. For application architecture, read [Building Osprey Web Apps with React and WebAssembly](/docs/web-apps/).

EOF
if [ -f "$DOCS_DIR/README.md" ]; then
    cat "$DOCS_DIR/README.md" >> "$DOCS_DIR/stdlib.md"
fi

echo "API reference documentation generated successfully!"
echo "Generated files:"
echo "  - $DOCS_DIR/stdlib.md (Main API Reference)"
echo "  - $DOCS_DIR/functions/ (Individual function docs)"
echo "  - $DOCS_DIR/types/ (Individual type docs)"
echo "  - $DOCS_DIR/operators/ (Individual operator docs)"
echo "  - $DOCS_DIR/keywords/ (Individual keyword docs)"
