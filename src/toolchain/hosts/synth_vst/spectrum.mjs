// [PRISM-ROUTING-VST] Drive Prism and measure the actual stereo PCM accepted by its device.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const [pluginFolder, scenario = 'dragonfly'] = process.argv.slice(2);
assert(pluginFolder && ['dragonfly', 'tal'].includes(scenario), 'Pass the VST3 folder and dragonfly or tal');
const root = process.cwd();
const output = join(root, 'target/pr-validation');
mkdirSync(output, { recursive: true });
const run = mkdtempSync(join(output, 'vst-spectrum-'));
const profile = join(run, 'profile');
mkdirSync(join(profile, '.prism'), { recursive: true });
writeFileSync(join(profile, '.prism/vst3-folder'), resolve(pluginFolder));
const worker = join(run, 'worker');
writeFileSync(worker, `#!/bin/bash
set -eo pipefail
case "$1" in
stream-session|audio-device)
    capture=$(mktemp "$CAPTURE/$1.XXXXXX")
    echo "$capture" >> "$CAPTURE/$1.order"
    tee "$capture.in" | "$VST_WORKER" "$@" | tee "$capture.out";;
*) exec "$VST_WORKER" "$@";;
esac
`, { mode: 0o755 });

const steps = [];
const press = invoke => steps.push({ invoke });
const wait = label => steps.push({ wait: label });
const type = (label, value) => steps.push({ type: [label, String(value)] });
const add = kind => type('Add component', kind);
function connect(source, target, port = 0) {
    press('Canvas output ' + source + ':0');
    press('Canvas input ' + target + ':' + port);
}
function edit(node, label, value) {
    press(node + ' ' + label);
    type(label, value);
}
function audition() {
    press('Audition Patch');
    wait('Preview finished.');
}
function insert(name, id) {
    wait(name);
    add('vst:' + id);
    wait(name + ' 0');
}
press('Patch Controls');
if (scenario === 'dragonfly') {
    const name = 'Dragonfly Room Reverb';
    insert(name, '2046504473616C633272666400000000');
    ['sine', 'stereo', 'stereo-output'].forEach(add);
    connect(1, 2);
    connect(2, 0);
    connect(0, 3);
    type('Module', 0);
    edit(name + ' 0', 'Early Level', 0);
    edit(name + ' 0', 'Late Level', 0);
    edit(name + ' 0', 'Dry Level', 1);
    audition();
    edit(name + ' 0', 'Dry Level', 0);
    audition();
    edit(name + ' 0', 'Late Level', 0.65);
    audition();
    press('Play Patch');
    wait('Playing the started patch. Stop and play again to apply edits.');
    steps.push({ frames: '48' });
    press('Stop Patch');
    wait('Playback stopped.');
} else {
    insert('TAL-NoiseMaker', '5653546E63757474616C2D6E6F697365');
    type('Module', 0);
    wait('TAL-NoiseMaker 0 Filter Cutoff');
    edit('TAL-NoiseMaker 0', 'Master Volume', 0.4);
    ['midi-note', 'sine', 'stereo', 'stereo-output'].forEach(add);
    type('Module', 2);
    edit('Oscillator 2', 'Frequency', 0);
    connect(2, 3);
    connect(3, 0);
    connect(1, 0, 1);
    connect(0, 4);
    audition();
    type('Module', 1);
    edit('MIDI notes 1', 'Pitch', 72);
    audition();
}
const journey = join(run, 'journey.json');
writeFileSync(journey, JSON.stringify({ steps }));
execFileSync('src/toolchain/hosts/gpui/target/release/osprey-gpui', [
    '--self-test', journey, run, '--headless', '--defer-processes', '--live-processes',
    '--machine', 'src/osprey/packages/signal_flow/signal_flow.machine.json', '--empty', '--vst-worker', worker,
], { stdio: 'inherit', timeout: 120000, env: { ...process.env, HOME: profile, CAPTURE: run,
    VST_WORKER: join(root, 'target/vst-worker/osprey-vst-worker') } });

const lines = file => readFileSync(file, 'utf8').trim().split('\n');
function pluginPcm(line) {
    const b = Buffer.from(line, 'hex');
    assert.equal(b.toString('ascii', 0, 4), 'OSV1');
    assert.equal(b.readUInt32LE(8), 2);
    const blocks = [];
    for (let offset = 16; offset < b.length;) {
        const frames = b.readUInt32LE(offset);
        assert(frames > 0 && frames <= 256 && offset + 8 + frames * 8 <= b.length);
        blocks.push(b.subarray(offset + 8, offset + 8 + frames * 8));
        offset += 8 + frames * 8;
    }
    return Buffer.concat(blocks);
}
function captured(plugin, device) {
    const pcm = Buffer.concat(lines(plugin + '.out').map(pluginPcm));
    const deviceBlocks = lines(device + '.in');
    assert.equal(deviceBlocks.pop(), '!drain');
    const wave = deviceBlocks.map(hex => Buffer.from(hex, 'hex')).map(b => {
        assert.equal(b.readUInt32LE(24), 48000);
        return b.subarray(b.indexOf('data') + 8);
    });
    assert(pcm.equals(Buffer.concat(wave)), 'Prism must deliver the plug-in output sample for sample');
    const receipts = lines(device + '.out').map(JSON.parse);
    assert(receipts.every(r => r.status === 0 && r.emptyCallbacks === 0));
    assert.equal(receipts.at(-1).returnedFrames, 12288);
    assert.equal(pcm.length, 12288 * 8);
    return pcm;
}
// Hann-windowed DFT at one-Hz intervals. Measure the held sound before MIDI note-off.
function spectrum(pcm, start, size, low, high) {
    const window = Array.from({ length: size }, (_, i) =>
        pcm.readFloatLE((start + i) * 8) * (1 - Math.cos(2 * Math.PI * i / (size - 1))));
    let peakHz = 0, amplitude = 0;
    for (let hz = low; hz <= high; hz++) {
        const angle = 2 * Math.PI * hz / 48000;
        let real = 0, imag = 0;
        for (let i = 0; i < size; i++) {
            real += window[i] * Math.cos(angle * i);
            imag += window[i] * Math.sin(angle * i);
        }
        const magnitude = 2 * Math.hypot(real, imag) / size;
        if (magnitude > amplitude) { amplitude = magnitude; peakHz = hz; }
    }
    return { peakHz, amplitude };
}
const plugins = lines(join(run, 'stream-session.order'));
const devices = lines(join(run, 'audio-device.order'));
assert.equal(plugins.length, scenario === 'tal' ? 2 : 4);
assert.equal(devices.length, plugins.length);
if (scenario === 'dragonfly') {
    plugins.pop();
    const receipts = lines(devices.pop() + '.out').map(JSON.parse);
    assert(receipts.every(r => r.status === 0 && r.emptyCallbacks === 0));
    assert(receipts.at(-1).returnedFrames >= 48000, 'Continuous Play must reach the device for at least a second');
    console.log('Live frames accepted without starvation:', receipts.at(-1).acceptedFrames);
}
const pcm = plugins.map((plugin, i) => captured(plugin, devices[i]));
const spectra = pcm.map(data => scenario === 'tal' ? spectrum(data, 800, 4096, 40, 1000)
    : spectrum(data, 4096, 8192, 100, 1000));
console.log(JSON.stringify({ run, scenario, spectra }, null, 2));
if (scenario === 'dragonfly') {
    const [dry, mute, wet] = spectra;
    assert.equal(dry.peakHz, 440);
    assert(dry.amplitude > 0.98 && dry.amplitude < 1.01);
    assert.equal(mute.amplitude, 0);
    for (let at = 0; at < pcm[1].length; at += 4) assert(pcm[1].readFloatLE(at) === 0, 'Nonzero muted sample');
    assert(Math.abs(wet.peakHz - 440) <= 2 && wet.amplitude > 0.01 && wet.amplitude < 0.25);
} else {
    const [low, high] = spectra;
    assert(low.amplitude > 0.01 && high.amplitude > 0.01);
    assert(Math.abs(high.peakHz / low.peakHz - 2) < 0.05, 'Raising MIDI pitch an octave doubles the measured fundamental');
}
