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

cat >"$scratch/golden.c" <<'C'
#include "golden.h"
#include <stdio.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 2 || chdir(argv[1]) != 0) { perror("golden working directory"); return 1; }
    return osprey_main();
}
C

goldens=(
    tests/core/arithmetic/calculator.test
    tests/core/collections/list_basics.test
    tests/core/collections/map_basics.test
    tests/regressions/basics/strings/string_pipeline.test
    tests/regressions/basics/json/json_document_query.test
    tests/regressions/basics/files/file_io_json_workflow.test
    tests/regressions/fiber/fiber_showcase.test
)
for base in "${goldens[@]}"; do
    for flavor in osp ospml; do
        "$compiler" "$base.$flavor" --target=ios-sim --compile -o "$scratch/golden.a"
        link_host iphonesimulator arm64-apple-ios15.0-simulator \
            "$scratch/golden.c" "$scratch/golden.a" "$scratch/golden"
        xcrun simctl spawn "$simulator" "$scratch/golden" "$scratch" >"$scratch/golden.stdout"
        diff -u "$base.osp.expectedoutput" "$scratch/golden.stdout"
        echo "==> iOS simulator golden passed: $base.$flavor"
    done
done

# Exercise the shared app logic, including Unicode limits, on the real target.
"$compiler" examples/mobile/inbox/test --target=ios-sim --compile -o "$scratch/golden.a"
link_host iphonesimulator arm64-apple-ios15.0-simulator \
    "$scratch/golden.c" "$scratch/golden.a" "$scratch/golden"
xcrun simctl spawn "$simulator" "$scratch/golden" "$scratch"
echo "==> iOS shared mobile domain suite passed"
