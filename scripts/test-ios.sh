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

cat >"$scratch/abi.osp" <<'OSPREY'
extern fn hostRecord(value: int, ready: bool, label: string) -> Unit
extern fn hostNegate(value: bool) -> bool
let seed = "persisted ${41}"
fn stored() = seed
fn greet(name: string) = "Hello ${name}"
fn invert(value) = !value
fn invertViaHost(value) = hostNegate(value)
fn big() = 9223372036854775807
fn scale(value: float) = value * 1.5
fn emit(value, ready, label) = hostRecord(value, ready, label)
hostRecord(41, true, "boot")
OSPREY

cat >"$scratch/abi.c" <<'C'
#include "abi.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static int calls;
void hostRecord(int64_t value, bool ready, const char *label) {
    assert(value == 41 && ready && strcmp(label, "boot") == 0);
    calls++;
}
bool hostNegate(bool value) { return !value; }
int main(void) {
    assert(osprey_main() == 0 && calls == 1);
    assert(osprey_big() == INT64_MAX && osprey_scale(4.0) == 6.0);
    assert(osprey_invert(false) && !osprey_invert(true));
    assert(osprey_invertViaHost(false) && !osprey_invertViaHost(true));
    assert(strcmp(osprey_stored(), "persisted 41") == 0);
    assert(strcmp(osprey_greet("Swift"), "Hello Swift") == 0);
    osprey_emit(41, true, "boot");
    assert(calls == 2 && strcmp(osprey_stored(), "persisted 41") == 0);
    puts("iOS C ABI passed");
    return 0;
}
C

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
printf 'iOS C ABI passed\n' >"$scratch/abi.expected"
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
