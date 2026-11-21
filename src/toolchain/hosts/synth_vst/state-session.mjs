// [PRISM-ROUTING-VST-WORKER] Restore edited opaque state in a fresh SDK plug-in process.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const [worker, bundle, id, inputs, parameter, initialDryLevel] = process.argv.slice(2);
const save = Buffer.from('SAVE').toString('hex');
function session(lines, success = true) {
    const run = spawnSync(worker, ['stream-session', bundle, id], {
        input: lines.join('\n') + '\n', encoding: 'utf8', timeout: 5000, maxBuffer: 4 * 1024 * 1024,
    });
    assert.ifError(run.error);
    if (success) assert.equal(run.status, 0, run.stderr);
    else assert.ok(run.status !== 0 && run.stderr.includes('state'), run.stderr);
    return run.stdout.trim().split('\n');
}
const frames = 64;
const edit = Buffer.alloc(28 + frames * Number(inputs) * 4);
edit.writeUInt32LE(frames);
edit.writeUInt32LE(1, 8);
edit.writeUInt32LE(Number(parameter), 12);
edit.writeUInt32LE(17, 16);
edit.writeDoubleLE(0.375, 20);
const [original, processed, changed] = session([save, edit.toString('hex'), save]);
assert.ok(processed.startsWith('4F535631'));
assert.notEqual(original, changed);
assert.deepEqual(session([changed, save, original, save]), ['4F4B', changed, '4F4B', original]);
const wrongClass = Buffer.from(changed, 'hex');
wrongClass[4] ^= 1;
session([wrongClass.toString('hex')], false);
session([changed.slice(0, 48)], false);

if (initialDryLevel !== undefined) {
    // Dragonfly's wet signal is delayed. Its first stereo frame measures the restored dry mix.
    assert.equal(Number(inputs), 2);
    const impulse = Buffer.alloc(12 + frames * 8);
    impulse.writeUInt32LE(frames);
    impulse.writeFloatLE(0.25, 12);
    impulse.writeFloatLE(-0.5, 16);
    for (const [state, gain] of [[original, Number(initialDryLevel)], [changed, 0.375]]) {
        const [ack, audio] = session([state, impulse.toString('hex')]);
        assert.equal(ack, '4F4B');
        const block = Buffer.from(audio, 'hex');
        assert.equal(block.toString('ascii', 0, 4), 'OSV1');
        assert.equal(block.readUInt32LE(8), 2);
        assert.equal(block.readUInt32LE(16), frames);
        assert.equal(block.readFloatLE(24), Math.fround(0.25 * gain));
        assert.equal(block.readFloatLE(28), Math.fround(-0.5 * gain));
    }
}
