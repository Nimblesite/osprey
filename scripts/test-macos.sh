#!/usr/bin/env bash
# Exercise the generated archive and header with Apple's C ABI directly on this
# Mac, using the fixture iOS and Android run. [IOS-TARGET-MACOS] [IOS-VERIFICATION]
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
compiler=${OSPREY_BIN:-$root/target/release/osprey}
scratch=$(mktemp -d "$root/compiler/bin/.macos-tests.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

cp scripts/mobile-abi.osp "$scratch/abi.osp"
cp scripts/mobile-abi.c "$scratch/abi.c"
"$compiler" "$scratch/abi.osp" --target=macos --compile -o "$scratch/abi.a"
xcrun --sdk macosx clang -target arm64-apple-macos12.0 \
    -isysroot "$(xcrun --sdk macosx --show-sdk-path)" \
    -O2 -std=c11 -Wall -Wextra -Werror "$scratch/abi.c" "$scratch/abi.a" -o "$scratch/abi"
"$scratch/abi" >"$scratch/abi.stdout"
printf 'Mobile C ABI passed\n' >"$scratch/abi.expected"
diff -u "$scratch/abi.expected" "$scratch/abi.stdout"
python3 scripts/test-mobile-tools.py --storage-host
OSPREY_BIN="$compiler" python3 scripts/test-mobile-tools.py --counter-host
echo "==> macOS scalar imports/exports and global lifetime passed"
