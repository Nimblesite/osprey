#!/bin/bash

# Generate reference documentation from the Osprey compiler (`osprey --docs`).
#
# The committed docs in src/docs/ are the source of truth for the website
# build. When a Rust compiler binary (target/release/osprey) is present AND
# supports the --docs flag, this script regenerates them; otherwise it keeps
# the committed docs and exits successfully so `npm run build` never requires
# a Rust toolchain.

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

# The Rust compiler does not implement --docs yet; detect support at runtime
# so this script starts regenerating automatically once the flag lands.
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
