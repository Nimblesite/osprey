// [DAW-TRANSPORT-LIVE-LATENCY] Exercise Prism's 160-frame request/receipt cadence on the real device.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';

const frames = 160;
const packet = Buffer.alloc(58 + frames * 8);
packet.write('RIFF');
packet.writeUInt32LE(packet.length - 8, 4);
packet.write('WAVEfmt ', 8);
packet.writeUInt32LE(18, 16);
packet.writeUInt16LE(3, 20);
packet.writeUInt16LE(2, 22);
packet.writeUInt32LE(48_000, 24);
packet.writeUInt32LE(384_000, 28);
packet.writeUInt16LE(8, 32);
packet.writeUInt16LE(32, 34);
packet.write('fact', 38);
packet.writeUInt32LE(4, 42);
packet.writeUInt32LE(frames, 46);
packet.write('data', 50);
packet.writeUInt32LE(frames * 8, 54);
const line = packet.toString('hex') + '\n';
const worker = spawn(process.argv[2], ['audio-device'], { stdio: ['pipe', 'pipe', 'inherit'], timeout: 12_000 });
const exit = once(worker, 'close');
const receipts = createInterface({ input: worker.stdout })[Symbol.asyncIterator]();
async function exchange(command) {
  worker.stdin.write(command);
  const receipt = await receipts.next();
  assert.equal(receipt.done, false, 'device ended before acknowledgement');
  return JSON.parse(receipt.value);
}

try {
  let receipt;
  for (let i = 1; i <= 900; ++i) {
    receipt = await exchange(line);
    assert.equal(receipt.acceptedFrames, i * frames);
    assert.equal(receipt.running, i >= 2);
    assert.ok(receipt.queuedFrames <= 960, 'steady playback must accept at most 20 ms ahead of a live edit');
  }
  assert.equal(receipt.emptyCallbacks, 0, 'the device ran empty with prepared audio available');
  const deadline = performance.now() + 3_000;
  do {
    receipt = await exchange('!status\n');
    if (receipt.emptyCallbacks > 0) break;
    await delay(10);
  } while (performance.now() < deadline);
  assert.ok(receipt.emptyCallbacks > 0, 'a deliberately late producer must register a queue drain');
  assert.equal(receipt.acceptedFrames, 900 * frames);
  assert.equal(receipt.queuedFrames, 0);
  receipt = await exchange(line);
  let recoveredDepth = 0;
  for (let i = 0; i < 120; ++i) {
    receipt = await exchange(line);
    recoveredDepth = Math.max(recoveredDepth, receipt.queuedFrames);
    assert.ok(receipt.queuedFrames <= 1440);
  }
  assert.ok(recoveredDepth > 960, 'a starved producer gets additional queue headroom');
  const recoveredEmpty = receipt.emptyCallbacks;
  for (let i = 0; i < 40; ++i) {
    receipt = await exchange(line);
    assert.ok(receipt.queuedFrames <= 960, 'recovered playback returns to the 20 ms limit');
    assert.equal(receipt.emptyCallbacks, recoveredEmpty, 'reducing queue depth must preserve stable playback');
  }
  receipt = await exchange('!drain\n');
  assert.equal(receipt.returnedFrames, 1061 * frames);
  assert.equal(receipt.queuedFrames, 0);
  assert.equal(receipt.running, false);
  worker.stdin.end();
  assert.deepEqual(await exit, [0, null]);
} finally {
  if (worker.exitCode === null) worker.kill();
}
