#!/usr/bin/env bash
# [SYNTH-DSP-PROOFS] Derive production FFT and spectral-frame theorems from this checkout.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/production-fft"
tools="$root/src/toolchain/benchmarks/proof/tools"
source="$root/src/osprey/packages/synth/src/ffts.ospml"
math="${source%/*}/spectral_math.ospml"
arrange="$root/src/osprey/examples/daw/src/arrange_math.ospml"
table="${source%/*}/table_math.ospml"
waves="${source%/*}/wavetables.ospml"
frames="${source%/*}/spectral_frames.ospml"
strike="${source%/*}/strike_combs.ospml"
allpass="${source%/*}/allpasses.ospml"
shaper="${source%/*}/waveshapers.ospml"
interpolation="${source%/*}/interpolation.ospml"
phase="${source%/*}/phase_operators.ospml"
phase_math="${source%/*}/phase_math.ospml"
grain="${source%/*}/grain_windows.ospml"
grain_math="${source%/*}/grain_math.ospml"
streams="${source%/*}/spectral_streams.ospml"
checker="$root/target/proof/openai/check-batch"
mkdir -p "$out"
reflect() {
    local file
    local bodies=()
    for file in "${source%/*}/delay_lines.ospml" "${source%/*}/fit_math.ospml" "${arrange%/*}/dial_math.ospml" "${arrange%/*}/canvas_geometry.ospml" "${arrange%/*}/timeline_geometry.ospml" "$arrange" "$table" "$waves" "$math" "$source" "$frames" "$strike" "$allpass" "${source%/*}/nonlinear_math.ospml" "$shaper" "$interpolation" "$phase_math" "$phase" "$grain_math" "$grain" "$streams"; do
        if [[ "$file" == "${1:-}" ]]; then file="$out/mutant.ospml"; fi
        bodies+=("$file")
    done
    cargo run --quiet --manifest-path "$root/src/toolchain/Cargo.toml" -p osprey-syntax --example synth_proof -- "${bodies[@]}" > "$out/bodies"
    awk -v body="$out/bodies" '/^-- INSERT PRODUCTION BODIES$/ {while ((getline line < body) > 0) print line; next} {print}' \
        "$tools/ProductionFft.lean.in" > "$out/ProductionFft.lean"
    cat "$tools/ProductionSpectral.lean.in" "$tools/ProductionTransform.lean.in" "$tools/ProductionWaveshaper.lean.in" >> "$out/ProductionFft.lean"
}
cd "$tools"
reflect
lake env lean --root="$out" -o "$out/ProductionFft.olean" "$out/ProductionFft.lean"
lean_path=$(lake env printenv LEAN_PATH)
exporter=$(lake env which lean4export)
check_export() {
    local backend=$1 file=$2 name=$3
    if [[ "$backend" == wasm ]]; then
        # The division lemma for the supported FFT sizes exceeds Node's default Wasm call stack.
        printf '%s\t%s\n' "$file" "$name" | node --stack-size=4096 "$root/scripts/proof/proof-batch-wasm.mjs" "$checker.wasm" "$out"
    else
        printf '%s\t%s\n' "$out/$file" "$name" | "$checker.$backend"
    fi
}
for theorem in validatedTransformSchedule negativeFrameRejected advertisedSizesAccepted shaperSymmetricInterval hardClipPositiveInterval tanhUpperPlateau upperClipRails wavefolderPair rotationComponents rotationUnitPower realSamplesConjugate addedDftBin grainAsrBoundarySamples grainAsrInterior windowedModulation windowedZeroIndex supportedTransformDepths transformSchedule overlapNecessary hannEnergyEighth hannEnergyQuarter delayPropagation delayFullTraversal heardTimeClamped dialMovement canvasGrabOffset envelopeSustain envelopeReleased stridedQuartet additiveSample additivePeriod rotationArithmetic combinedCorrect quartetCorrect conjugateComponents conjugateTwice combinedDftBins scaledComponents scaledDftBin rotationConjugate strikeDifference strikePeriodicNull allpassStateStep allpassFactoredStep allpassTransfer allpassSteadyMagnitude shaperBypass shaperMidpoint shaperQuotient shaperIntervalAverage shaperCurvePair shaperPrimitiveOffset shaperConstantSegment interpolationEndpoints interpolationInterior interpolationWeights interpolationConstant wavePulseZeroIndices phasePulse wavePulseSegments grainHannEndpoints grainHannInterior grainHannBoundarySamples overlapNormalization overlapReconstructs; do
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
    reflect "$input"
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
reject_edit negativeFrameRejected 's/listContains sizes size/listContains sizes 64/'
reject_edit advertisedSizesAccepted 's/listContains sizes size/listContains [] size/'
reject_edit shaperSymmetricInterval 's/x \* x \* x \* x \/ 12.0/x * x * x \/ 12.0/' "${source%/*}/nonlinear_math.ospml"
reject_edit hardClipPositiveInterval 's/false => a - 0.5/false => a * 0.5/' "${source%/*}/nonlinear_math.ospml"
reject_edit tanhUpperPlateau 's/true => 20.0/true => x/' "${source%/*}/nonlinear_math.ospml"
reject_edit upperClipRails 's/match x >= 1.0/match x <= 1.0/' "${source%/*}/nonlinear_math.ospml"
reject_edit wavefolderPair 's/0.6366197723675814/0.6366197723675815/' "${source%/*}/nonlinear_math.ospml"
reject_edit rotationComponents 's/re \* re + im \* im/re * re - im * im/' "$math"
reject_edit rotationComponents 's/false => floatDiv (im, m)/false => floatDiv (re, m)/' "$math"
reject_edit realSamplesConjugate 's/im = 0.0 - value.im/im = value.im/'
reject_edit addedDftBin 's/im = e.im + t.im/im = e.im - t.im/'
reject_edit grainAsrBoundarySamples 's/age >= duration - 1.0/age > duration - 1.0/' "$grain_math"
reject_edit grainAsrInterior 's/floatDiv (duration - 1.0 - age, release/floatDiv (duration - 1.0 - age, attack/' "$grain_math"
reject_edit phasePulse 's/sin (3.141592653589793 \* t)/cos (3.141592653589793 * t)/' "$phase_math"
reject_edit windowedModulation 's/6.283185307179586 \* ratio \* phase/6.283185307179586 * phase * ratio/' "$phase_math"
reject_edit supportedTransformDepths 's/2048, 4096]/2048, 2048]/'
reject_edit transformSchedule 's/step = satAdd stride stride/step = satAdd stride 1/'
reject_edit transformSchedule 's/listConcat halves.lower halves.upper/listConcat halves.upper halves.lower/'
reject_edit hannEnergyQuarter 's/0.5 - 0.5 [*] cos/0.5 - 1.0 * cos/' "$math"
reject_edit delayPropagation 's/listAppend rest sample/listPrepend rest sample/' "${source%/*}/delay_lines.ospml"
reject_edit heardTimeClamped 's/floatDiv (returned, rate), floatDiv (accepted, rate)/floatDiv (accepted, rate), floatDiv (returned, rate)/' "${arrange%/*}/timeline_geometry.ospml"
reject_edit dialMovement 's/abs (y - startY) >= 2.0/abs (y - startY) >= 1.0/' "${arrange%/*}/dial_math.ospml"
reject_edit canvasGrabOffset 's/= pointer - centre/= centre - pointer/' "${arrange%/*}/canvas_geometry.ospml"
reject_edit envelopeReleased 's/frame >= gate + release/frame >= gate + attack/' "$arrange"
reject_edit envelopeSustain 's/attack [*] sampleRate, release [*] sampleRate/release * sampleRate, attack * sampleRate/' "$arrange"
reject_edit stridedQuartet 's/third = satAdd second stride/third = satAdd first stride/'
reject_edit stridedQuartet 's/quartet (a, b, c, d, sign))/quartet (a, c, b, d, sign))/'
reject_edit additiveSample 's/number [*] position + phase/number * phase + position/' "$table"
reject_edit additiveSample 's/h.amplitude, toFloat h.number, h.phase/h.phase, toFloat h.number, h.amplitude/' "$waves"
reject_edit rotationArithmetic 's/ar [*] br - ai [*] bi/ar * br + ai * bi/' "$math"
reject_edit rotationArithmetic 's/ar [*] bi + ai [*] br/ar * br + ai * bi/' "$math"
reject_edit rotationArithmetic 's/floatDiv (sign [*] 6.283185307179586 [*] index, size)/floatDiv (sign * 6.283185307179586 * size, index)/' "$math"
reject_edit combinedCorrect 's/listPrepend rest.upper (subtracted (e, t))/listPrepend rest.upper (added (e, t))/'
reject_edit quartetCorrect 's/added (upper, difference), subtracted (lower, sum)/subtracted (lower, sum), added (upper, difference)/'
reject_edit conjugateComponents 's/im = 0.0 - value.im/im = value.im/'
reject_edit scaledComponents 's/im = value.im [*] gain/im = value.re * gain/' "$frames"
reject_edit strikeDifference 's/delay config + 1.0/delay config + 0.0/' "$strike"
reject_edit allpassStateStep 's/held.input - coefficient/held.input + coefficient/' "$allpass"
reject_edit allpassStateStep 's/input = input, output = value/input = held.input, output = value/' "$allpass"
reject_edit shaperMidpoint 's/current + previous/current - previous/' "$shaper"
reject_edit shaperCurvePair 's/HardCurve => NonlinearMath::hardIntegral/HardCurve => NonlinearMath::cubicIntegral/' "$shaper"
reject_edit interpolationEndpoints 's/true => b$/true => a/' "$interpolation"
reject_edit wavePulseSegments 's@phase / breakpoint, first@phase / breakpoint, second@' "$phase"
reject_edit wavePulseSegments 's/breakpoint), second/breakpoint), first/' "$phase"
reject_edit grainHannInterior 's/last = duration - 1.0/last = duration/' "$grain"
reject_edit overlapNormalization 's/8.0 [*] hop/4.0 * hop/' "$streams"
reject_edit overlapReconstructs 's/sum + value [*] scale/sum - value * scale/' "$streams"
# A source helper must never be silently interpreted as a primitive or as a
# similarly named function from another module.
reject_binding() {
    local expected=$1
    if cargo run --quiet --manifest-path "$root/src/toolchain/Cargo.toml" -p osprey-syntax --example synth_proof -- "$out/mutant.ospml" >"$out/scope.log" 2>&1; then
        echo "accepted invalid proof mapping: $expected" >&2
        return 1
    fi
    grep -Fqx "Error: \"$expected\"" "$out/scope.log"
    echo "REJECT $expected"
}
awk '{print} /^module PhaseMath$/ {print "    sin t = cos t"}' "$phase_math" > "$out/mutant.ospml"
reject_binding "module declaration shadows a reflected primitive: PhaseMath.sin"
awk '{sub(/cos \(/, "window ("); print} /^module GrainWindows$/ {print "    window t = sin t"}' "$grain" > "$out/mutant.ospml"
reject_binding "unreflected local call GrainWindows.window"
reflect

# Replay an unchanged proof certificate against a different, well-typed
# production operation. The Osprey kernel itself must reject the stale proof.
node --input-type=module - "$out" <<'JS'
import fs from "node:fs";
const dir = process.argv[2];
const rows = fs.readFileSync(dir + "/combinedCorrect.ndjson", "utf8").trimEnd().split("\n").map(JSON.parse);
const namespace = rows.find(r => r.str?.pre === 0 && r.str.str === "ProductionFft").in;
const prefix = rows.find(r => r.str?.pre === namespace && r.str.str === "Ffts").in;
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
