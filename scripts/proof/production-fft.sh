#!/usr/bin/env bash
# [SYNTH-DSP-PROOFS] Derive production FFT and spectral-frame theorems from this checkout.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/production-fft"
tools="$root/src/toolchain/benchmarks/proof/tools"
source="$root/src/osprey/packages/synth/src/ffts.ospml"
frames="${source%/*}/spectral_frames.ospml"
checker="$root/target/proof/openai/check-batch"
mkdir -p "$out"
reflect() {
    cargo run --quiet --manifest-path "$root/src/toolchain/Cargo.toml" -p osprey-syntax --example synth_proof -- "$1" "$2" > "$out/bodies"
    awk -v body="$out/bodies" '/^-- INSERT PRODUCTION BODIES$/ {while ((getline line < body) > 0) print line; next} {print}' \
        "$tools/ProductionFft.lean.in" > "$out/ProductionFft.lean"
}
cd "$tools"
reflect "$source" "$frames"
lake env lean --root="$out" -o "$out/ProductionFft.olean" "$out/ProductionFft.lean"
lean_path=$(lake env printenv LEAN_PATH)
exporter=$(lake env which lean4export)
check_export() {
    local backend=$1 file=$2 name=$3
    if [[ "$backend" == wasm ]]; then
        printf '%s\t%s\n' "$file" "$name" | node "$root/scripts/proof/proof-batch-wasm.mjs" "$checker.wasm" "$out"
    else
        printf '%s\t%s\n' "$out/$file" "$name" | "$checker.$backend"
    fi
}
for theorem in combinedCorrect quartetCorrect conjugateComponents conjugateTwice combinedDftBins scaledComponents scaledDftBin rotationConjugate; do
    name="ProductionFft.$theorem"
    LEAN_PATH="$out:$lean_path" "$exporter" ProductionFft -- "$name" > "$out/$theorem.ndjson"
    for backend in gc arc wasm; do
        result=$(check_export "$backend" "$theorem.ndjson" "$name")
        echo "$backend $result"
        test "$result" = "PASS $name | axioms: none"
    done
done

# Feed real source edits through the same parser. A syntax/translation error
# is an infrastructure failure; only rejection at the named theorem counts.
reject_edit() {
    local theorem=$1 rewrite=$2 input=${3:-$source}
    sed "$rewrite" "$input" > "$out/mutant.ospml"
    if cmp -s "$input" "$out/mutant.ospml"; then
        echo "mutation did not change $input" >&2
        return 1
    fi
    if [[ "$input" == "$source" ]]; then reflect "$out/mutant.ospml" "$frames"; else reflect "$source" "$out/mutant.ospml"; fi
    if lake env lean --root="$out" "$out/ProductionFft.lean" > "$out/rejected.log" 2>&1; then
        echo "accepted broken production $theorem" >&2
        return 1
    fi
    local start end
    start=$(grep -n "^theorem $theorem " "$out/ProductionFft.lean" | cut -d: -f1)
    end=$(awk -v start="$start" 'NR > start && /^(theorem |end )/ {print NR; exit}' "$out/ProductionFft.lean")
    awk -F: -v start="$start" -v end="$end" '/ProductionFft.lean:[0-9]+:[0-9]+: error:/ && $2 >= start && $2 < end { rejected=1 } END {exit !rejected}' "$out/rejected.log"
    echo "REJECT $theorem production mutation"
}
reject_edit combinedCorrect 's/listPrepend rest.upper (subtracted (e, t))/listPrepend rest.upper (added (e, t))/'
reject_edit quartetCorrect 's/added (upper, difference), subtracted (lower, sum)/subtracted (lower, sum), added (upper, difference)/'
reject_edit conjugateComponents 's/im = 0.0 - value.im/im = value.im/'
reject_edit scaledComponents 's/im = value.im [*] gain/im = value.re * gain/' "$frames"
reflect "$source" "$frames"

# Replay an unchanged proof certificate against a different, well-typed
# production operation. The Osprey kernel itself must reject the stale proof.
node --input-type=module - "$out" <<'JS'
import fs from "node:fs";
const dir = process.argv[2];
const rows = fs.readFileSync(dir + "/combinedCorrect.ndjson", "utf8").trimEnd().split("\n").map(JSON.parse);
const prefix = rows.find(r => r.str?.pre === 0 && r.str.str === "ProductionFft").in;
function definition(name) {
    const id = rows.find(r => r.str?.pre === prefix && r.str.str === name).in;
    return rows.find(r => r.def?.name === id).def;
}
const add = definition("added"), subtract = definition("subtracted");
if (add.type !== subtract.type || add.value === subtract.value) throw Error("expected distinct butterfly operations of the same type");
subtract.value = add.value;
fs.writeFileSync(dir + "/stale.ndjson", rows.map(r => JSON.stringify(r)).join("\n") + "\n");
JS
name=ProductionFft.combinedCorrect
for backend in gc arc wasm; do
    result=$(check_export "$backend" stale.ndjson "$name")
    [[ "$result" == "FAIL $name: kernel: type mismatch:"* ]] || { echo "$result" >&2; exit 1; }
    echo "$backend REJECT stale production butterfly proof"
done
