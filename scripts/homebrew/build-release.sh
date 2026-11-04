#!/bin/bash
set -e

VERSION=${1:-"0.1.0"}
echo "🚀 Creating Osprey v$VERSION Homebrew release..."

# Build the compiler (C runtime archives + Rust workspace) from the repo root
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
echo "🔨 Building Osprey..."
make build

# Create release tarball
echo "📦 Creating release tarball..."
mkdir -p scripts/homebrew/release
cp target/release/osprey scripts/homebrew/release/
cp src/compiler/lib/lib*.a scripts/homebrew/release/

cd scripts/homebrew/release
tar -czf "osprey-$VERSION.tar.gz" osprey lib*.a
SHA256=$(shasum -a 256 "osprey-$VERSION.tar.gz" | cut -d' ' -f1)
echo "✅ SHA256: $SHA256"

# Create GitHub release
echo "🚀 Creating GitHub release..."
cd "$repo_root"
gh release create "v$VERSION" \
    --title "Osprey v$VERSION" \
    --notes "Osprey compiler v$VERSION" \
    --repo "ChristianFindlay/osprey" \
    "scripts/homebrew/release/osprey-$VERSION.tar.gz"

echo "✅ Done! Tarball uploaded with SHA256: $SHA256"
echo "🍺 Update the osprey.rb manually with the new URL and SHA256"

# Clean up
rm -rf scripts/homebrew/release
