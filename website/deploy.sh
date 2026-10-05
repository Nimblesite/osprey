#!/bin/bash

set -euo pipefail

echo "🚀 Deploying Osprey Website"

# Check if we're in the website directory
if [ ! -f "package.json" ]; then
    echo "❌ Error: Must be run from the website directory"
    exit 1
fi

# The Rust compiler binary (cargo build --release) is optional: when present
# and it supports --docs, scripts/generate-docs.sh (run by `npm run build`)
# regenerates the API reference; otherwise the committed docs are used.
COMPILER_PATH="../target/release/osprey"
if [ -f "$COMPILER_PATH" ]; then
    echo "✅ Found Osprey compiler at $COMPILER_PATH"
else
    echo "ℹ️ No compiler at $COMPILER_PATH — using committed docs (build with: cargo build --release)"
fi

# Install dependencies if needed
if [ ! -d "node_modules" ]; then
    echo "📦 Installing dependencies..."
    npm install
fi

# Build the WebAssembly demo assets that the /wasm/ site page publishes.
echo "🧱 Building WebAssembly demo assets..."
(cd .. && make wasm-site)

# Build the website (runs update-playground + generate-docs + eleventy)
echo "🏗️ Building website..."
npm run build

echo "✅ Website built successfully!"
echo "📁 Output directory: _site/"

# If we're in GitHub Actions, the deployment will be handled by the workflow
if [ "${GITHUB_ACTIONS:-false}" = "true" ]; then
    echo "🔄 Running in GitHub Actions - deployment will be handled by workflow"
else
    echo "💡 To serve locally, run: npm run start"
    echo "💡 Website is ready for deployment from the _site/ directory"
fi
