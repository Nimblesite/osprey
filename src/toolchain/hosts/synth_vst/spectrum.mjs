// [PRISM-ROUTING-VST] Drive Prism and measure the actual stereo PCM accepted by its device.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const [pluginFolder, scenario = 'dragonfly', mode = 'headless'] = process.argv.slice(2);
assert(pluginFolder && ['dragonfly', 'tal', 'patch', 'latency'].includes(scenario), 'Pass the VST3 folder and dragonfly, tal, patch or latency');
assert(['headless', 'window'].includes(mode), 'Choose headless or window');
const paired = scenario === 'patch';
const reverbName = 'Dragonfly Room Reverb';
const reverbId = '2046504473616C633272666400000000';
const root = process.cwd();
const output = join(root, 'target/pr-validation');
mkdirSync(output, { recursive: true });
const run = mkdtempSync(join(output, 'vst-spectrum-'));
const profile = join(run, 'profile');
mkdirSync(join(profile, '.prism'), { recursive: true });
const preference = join(profile, '.prism/vst3-folder');
const emptyFolder = join(run, 'empty-plugins');
mkdirSync(emptyFolder);
writeFileSync(preference, emptyFolder);
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
function playAndStop(frames) {
    press('Play Patch');
    wait('Playing the patch.');
    steps.push({ frames: String(frames) });
    press('Stop Patch');
    wait('Playback stopped.');
}
function audition() {
    press('Audition Patch');
    wait('Preview finished.');
}
function insert(name, id, node = 0) {
    wait(name);
    add('vst:' + id);
    wait(name + ' ' + node);
}
press('Patch Controls');
if (scenario === 'latency') {
    insert('CHOWTapeModel', 'ABCDEF019182FAEB43686F774A646F78');
    ['sine', 'stereo', 'split', 'mixer', 'stereo', 'stereo-output'].forEach(add);
    connect(1, 2);
    connect(2, 0);
    connect(0, 3);
    connect(3, 4);
    connect(1, 4, 1);
    connect(4, 5);
    connect(5, 6);
    type('Module', 4);
    edit('Mixer 4', 'First input gain', 0.25);
    edit('Mixer 4', 'Second input gain', 0.25);
    audition();
} else if (scenario === 'dragonfly') {
    const name = reverbName;
    insert(name, reverbId);
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
    edit(name + ' 0', 'Late Level', 0);
    edit(name + ' 0', 'Dry Level', 0.375);
    add('lfo');
    press('Canvas output 4:0');
    type('Module', 0);
    press('Modulate Dry Level');
    wait('Cable 4:0 to 0:1');
    press('Play Patch');
    wait('Playing the patch.');
    steps.push({ frames: '24' });
    press('Open plug-in window');
    wait('Focus plug-in window');
    press('Focus plug-in window');
    press('Cable 4:0 to 0:1');
    press('Delete selected cable');
    steps.push({ gone: 'Cable 4:0 to 0:1' }, { frames: '36' });
    edit(name + ' 0', 'Dry Level', 0);
    steps.push({ frames: '48' });
    edit(name + ' 0', 'Dry Level', 1);
    press('Close plug-in window');
    wait('Open plug-in window');
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
    if (paired) {
        insert(reverbName, reverbId, 5);
        type('Module', 5);
        edit(reverbName + ' 5', 'Dry Level', 0.55);
        edit(reverbName + ' 5', 'Late Level', 0.65);
        edit(reverbName + ' 5', 'Decay', 0.45);
        connect(0, 5);
        connect(5, 4);
        wait('Cable 5:0 to 4:0');
    } else {
        connect(0, 4);
    }
    audition();
    type('Module', 1);
    edit('MIDI notes 1', 'Pitch', 72);
    audition();
    if (paired) {
        steps.push({ context: 'Canvas ' + reverbName + ' 5' });
        press('Delete node');
        steps.push({ gone: 'Canvas ' + reverbName + ' 5' }, { gone: 'Cable 0:0 to 5:0' }, { gone: 'Cable 5:0 to 4:0' });
        wait('Canvas TAL-NoiseMaker 0');
        press('Undo');
        wait('Cable 0:0 to 5:0');
        wait('Cable 5:0 to 4:0');
        type('Module', 1);
        edit('MIDI notes 1', 'Gate (frames)', 48000);
        playAndStop(mode === 'window' ? 960 : 85);
    }
}
function replay(name, actions) {
    const journey = join(run, name + '.json');
    writeFileSync(journey, JSON.stringify({ steps: actions }));
    execFileSync('src/toolchain/hosts/gpui/target/release/osprey-gpui', [
        '--self-test', journey, run, ...(mode === 'headless' ? ['--headless'] : []), '--defer-processes', '--live-processes',
        '--machine', 'src/osprey/packages/signal_flow/signal_flow.machine.json', '--empty', '--vst-worker', worker,
    ], { stdio: 'inherit', timeout: 120000, env: { ...process.env, HOME: profile, CAPTURE: run,
        VST_WORKER: join(root, 'target/vst-worker/osprey-vst-worker') } });
}

const pluginName = scenario === 'latency' ? 'CHOWTapeModel' : scenario === 'dragonfly' ? reverbName : 'TAL-NoiseMaker';
replay('settings', [
    { invoke: 'Patch Controls' }, { gone: pluginName },
    { invoke: 'Settings…' }, { type: ['VST3 folder', resolve(pluginFolder)] },
    { invoke: 'Save settings' }, { wait: pluginName },
]);
assert.equal(readFileSync(preference, 'utf8'), resolve(pluginFolder));
replay('journey', steps);

const lines = file => readFileSync(file, 'utf8').trim().split('\n');
function pluginPcm(line, latency) {
    const b = Buffer.from(line, 'hex');
    assert.equal(b.toString('ascii', 0, 4), 'OSV1');
    assert.equal(b.readUInt32LE(8), 2);
    const blocks = [];
    for (let offset = 16; offset < b.length;) {
        const frames = b.readUInt32LE(offset);
        assert(frames > 0 && frames <= 256 && offset + 8 + frames * 8 <= b.length);
        if (latency !== undefined) assert.equal(b.readUInt32LE(offset + 4), latency);
        blocks.push(b.subarray(offset + 8, offset + 8 + frames * 8));
        offset += 8 + frames * 8;
    }
    return Buffer.concat(blocks);
}
function effectInput(line, onParameter) {
    const packet = Buffer.from(line, 'hex');
    const blocks = [];
    for (let offset = 0; offset < packet.length;) {
        const frames = packet.readUInt32LE(offset);
        const notes = packet.readUInt32LE(offset + 4);
        const parameters = packet.readUInt32LE(offset + 8);
        offset += 12 + notes * 24;
        for (let i = 0; i < parameters; i++, offset += 16) {
            if (onParameter) onParameter(packet.readUInt32LE(offset), packet.readUInt32LE(offset + 4), packet.readDoubleLE(offset + 8));
        }
        const bytes = frames * 8;
        assert(frames > 0 && frames <= 256 && offset + bytes <= packet.length);
        blocks.push(packet.subarray(offset, offset + bytes));
        offset += bytes;
    }
    return Buffer.concat(blocks);
}
function pluginOutput(session) {
    return Buffer.concat(lines(session + '.out').map(line => pluginPcm(line)));
}
function verifyCable(instrument, effect) {
    const input = Buffer.concat(lines(effect + '.in').map(line => effectInput(line)));
    const dry = pluginOutput(instrument);
    assert(input.length > 0 && input.equals(dry.subarray(0, input.length)), 'Stereo instrument output must reach the effect unchanged');
    return dry;
}
function deviceAudio(device, drain) {
    const packets = lines(device + '.in');
    if (drain) assert.equal(packets.pop(), '!drain');
    return Buffer.concat(packets.map(hex => {
        const b = Buffer.from(hex, 'hex');
        assert.equal(b.readUInt32LE(24), 48000);
        const header = b.indexOf('data');
        assert(header > 0 && b.readUInt32LE(header + 4) === b.length - header - 8);
        return b.subarray(header + 8);
    }));
}
// Preserve the device's float WAV format while combining its blocks into one render.
function renderWave(device, pcm) {
    const first = Buffer.from(lines(device + '.in')[0], 'hex');
    const data = first.indexOf('data');
    const fact = first.indexOf('fact');
    assert.equal(first.toString('ascii', 8, 12), 'WAVE');
    assert.equal(first.readUInt16LE(20), 3);
    assert.equal(first.readUInt16LE(22), 2);
    assert(fact >= 12 && fact + 12 <= data);
    const header = Buffer.from(first.subarray(0, data + 8));
    header.writeUInt32LE(header.length + pcm.length - 8, 4);
    header.writeUInt32LE(pcm.length / 8, fact + 8);
    header.writeUInt32LE(pcm.length, data + 4);
    const path = join(run, 'Prism-TAL-Dragonfly.wav');
    writeFileSync(path, Buffer.concat([header, pcm]));
    const rendered = readFileSync(path);
    assert.equal(rendered.length - 8, rendered.readUInt32LE(4));
    assert(rendered.subarray(header.length).equals(pcm));
    return rendered.subarray(header.length);
}
function tailRms(data, channel) {
    let sum = 0;
    for (let i = 96000; i < 120000; i++) sum += data.readFloatLE(i * 8 + channel * 4) ** 2;
    return Math.sqrt(sum / 24000);
}
function lastDeviceReceipt(device) {
    const receipts = lines(device + '.out').map(JSON.parse);
    assert(receipts.every(r => r.status === 0 && r.emptyCallbacks === 0));
    return receipts.at(-1);
}
function captured(plugin, device) {
    const pcm = pluginOutput(plugin);
    assert(pcm.equals(deviceAudio(device, true)), 'Prism must deliver the plug-in output sample for sample');
    const receipt = lastDeviceReceipt(device);
    assert.equal(receipt.returnedFrames, 12288);
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
if (scenario === 'latency') {
    assert.equal(devices.length, 1);
    assert.equal(plugins.length, 1);
    const lag = 39; // CHOW Tape Model 2.11.4 reports 39 frames at 48 kHz.
    const wet = Buffer.concat(lines(plugins[0] + '.out').map(line => pluginPcm(line, lag)));
    const dry = Buffer.concat(lines(plugins[0] + '.in').map(line => effectInput(line)));
    const mix = deviceAudio(devices[0], true);
    assert.equal(mix.length, wet.length);
    assert.equal(dry.length, wet.length);
    assert(mix.length >= 12288 * 8 && wet.some(byte => byte !== 0));
    for (let frame = 0; frame < mix.length / 8; frame++) {
        const source = frame < lag ? 0 : dry.readFloatLE(8 * (frame - lag));
        const expected = (source + wet.readFloatLE(8 * frame)) * 0.25;
        for (const channel of [0, 1]) assert(Math.abs(mix.readFloatLE(8 * frame + 4 * channel) - expected) < 2e-6, 'Misaligned parallel paths at frame ' + frame);
    }
    console.log('Verified', mix.length / 8, 'stereo frames against the wet output and the delayed dry source.');
    process.exit(0);
}
assert.equal(devices.length, paired ? 3 : scenario === 'tal' ? 2 : 4);
assert.equal(plugins.length, devices.length * (paired ? 2 : 1));
if (scenario !== 'tal') {
    const device = devices.pop();
    const output = plugins.pop();
    const receipt = lastDeviceReceipt(device);
    const frames = receipt.acceptedFrames;
    const minimumSeconds = paired && mode === 'window' ? 10 : 1;
    assert(receipt.returnedFrames >= minimumSeconds * 48000, `Device must play ${minimumSeconds} seconds before Stop`);
    if (paired) {
        const dry = verifyCable(plugins.pop(), output);
        const pcm = deviceAudio(device, false);
        assert.equal(pcm.length, frames * 8);
        assert(pcm.equals(pluginOutput(output).subarray(0, pcm.length)));
        const rendered = renderWave(device, pcm);
        const held = spectrum(rendered, 8192, 8192, 40, 1000);
        assert(Math.abs(held.peakHz - 131) <= 2 && held.amplitude > 0.01);
        for (const channel of [0, 1]) {
            const tail = tailRms(rendered, channel);
            assert(tail > 0.00001 && tail > 2 * tailRms(dry, channel), 'Reverb tail must survive note-off');
        }
    }
    if (scenario === 'dragonfly') {
        const dryPoints = [];
        for (const line of lines(output + '.in')) {
            effectInput(line, (id, offset, value) => {
                if (id === 2) dryPoints.push([offset, value]);
            });
        }
        assert(dryPoints.slice(0, -3).some(([offset, value]) => offset === 128 && value > 0 && value < 1), 'The LFO must drive Dry Level');
        assert.deepEqual(dryPoints.slice(-3), [[0, 0.375], [0, 0], [0, 1]], 'Unplug restores the stored dial and releases it for live editing');
        // Device receipts prove the same process stayed alive across both edits.
        const pcm = deviceAudio(device, false);
        let heard = false, quiet = 0, muted = false, recovered = false;
        for (let i = 0; i < pcm.length; i += 8) {
            const level = Math.abs(pcm.readFloatLE(i));
            if (level > 0.1) {
                heard = true;
                if (muted) recovered = true;
            }
            quiet = level === 0 && heard ? quiet + 1 : 0;
            if (quiet >= 2048) muted = true;
        }
        assert(heard && muted && recovered, 'Live dry level edits must mute and restore the sounding effect');
    }
    console.log('Live frames accepted without starvation:', frames);
}
const pcm = devices.map((device, i) => {
    const output = paired ? plugins[2 * i + 1] : plugins[i];
    if (paired) verifyCable(plugins[2 * i], output);
    return captured(output, device);
});
const spectra = pcm.map(data => scenario !== 'dragonfly' ? spectrum(data, 800, 4096, 40, 1000)
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
