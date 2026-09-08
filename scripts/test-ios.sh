#!/usr/bin/env bash
# Exercise the generated archive and header with Apple's real C ABI, then run
# existing language goldens in an iPhone simulator. [IOS-VERIFICATION]
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
compiler=${OSPREY_BIN:-$root/target/release/osprey}
simulator=$(bash scripts/ios-simulator.sh)
scratch=$(mktemp -d "$root/compiler/bin/.ios-tests.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

cp scripts/mobile-abi.osp "$scratch/abi.osp"
cp scripts/mobile-abi.c "$scratch/abi.c"

link_host() {
    local sdk=$1 triple=$2 source=$3 archive=$4 executable=$5
    xcrun --sdk "$sdk" clang -target "$triple" \
        -isysroot "$(xcrun --sdk "$sdk" --show-sdk-path)" \
        -O2 -std=c11 -Wall -Wextra -Werror "$source" "$archive" -o "$executable"
}

for target in ios ios-sim; do
    "$compiler" "$scratch/abi.osp" --target="$target" --compile -o "$scratch/abi.a"
    sdk=iphoneos
    triple=arm64-apple-ios15.0
    if [[ $target == ios-sim ]]; then sdk=iphonesimulator; triple+=-simulator; fi
    link_host "$sdk" "$triple" "$scratch/abi.c" "$scratch/abi.a" "$scratch/abi-$target"
done

xcrun simctl spawn "$simulator" "$scratch/abi-ios-sim" >"$scratch/abi.stdout"
printf 'Mobile C ABI passed\n' >"$scratch/abi.expected"
diff -u "$scratch/abi.expected" "$scratch/abi.stdout"
echo "==> iOS device C host linked; simulator scalar imports/exports and global lifetime passed"

# The seven hand-picked goldens that used to run here are gone, and nothing is
# lost: `OSPREY_TARGET=ios-sim crates/run_test_corpus.sh` now runs the WHOLE
# corpus on the simulator against the same byte-exact goldens, with its
# rejections pinned in tests/IOS_UNPORTABLE.txt. Running fourteen of those
# programs a second time here would double the slowest stage to observe a
# strict subset of what the harness already observed.

cat >"$scratch/golden.c" <<'C'
#include "golden.h"
#include <stdio.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 2 || chdir(argv[1]) != 0) { perror("golden working directory"); return 1; }
    return osprey_main();
}
C

# Exercise the shared app logic, including Unicode limits, on the real target.
"$compiler" examples/mobile/inbox/test --target=ios-sim --compile -o "$scratch/golden.a"
link_host iphonesimulator arm64-apple-ios15.0-simulator \
    "$scratch/golden.c" "$scratch/golden.a" "$scratch/golden"
xcrun simctl spawn "$simulator" "$scratch/golden" "$scratch"
echo "==> iOS shared mobile domain suite passed"
