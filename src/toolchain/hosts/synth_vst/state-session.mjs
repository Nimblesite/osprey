// [PRISM-ROUTING-VST-WORKER] Restore edited opaque state in a fresh SDK plug-in process.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const [worker, bundle, id, inputs, parameter] = process.argv.slice(2);
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
