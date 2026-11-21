// [DAW-TRANSPORT-LIVE-SPEED] Measure the production live encoder with an empty note cache.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';

const [renderer, song, from = '0', beats = '2'] = process.argv.slice(2);
const folder = await mkdtemp(join(tmpdir(), 'daw-playback-rate-'));
try {
  const start = performance.now();
  const player = spawn(renderer, [song, folder, from, beats, 'live'], {
    stdio: ['ignore', 'pipe', 'inherit'], timeout: 300_000,
  });
  const exited = new Promise((resolve) => {
    player.once('error', (error) => resolve(error.message));
    player.once('close', (code, signal) => resolve(signal ?? code));
  });
  let frames = 0, firstMs = 0, worstGapMs = 0, last = start;
  const hash = createHash('sha256');
  for await (const line of createInterface({ input: player.stdout })) {
    const now = performance.now();
    const wav = Buffer.from(line, 'hex');
    if (wav.toString('ascii', 0, 4) !== 'RIFF' || wav.toString('ascii', 8, 12) !== 'WAVE' ||
        wav.readUInt16LE(20) !== 3 || wav.readUInt16LE(22) !== 2 ||
        wav.readUInt32LE(24) !== 48_000 || wav.readUInt16LE(34) !== 32) {
      throw Error('Expected a stereo 48 kHz float32 WAV block.');
    }
    const data = wav.indexOf('data', 36);
    if (data < 0 || wav.readUInt32LE(data + 4) !== wav.length - data - 8 || (wav.length - data - 8) % 8) {
      throw Error('Incomplete PCM block.');
    }
    if (frames) worstGapMs = Math.max(worstGapMs, now - last);
    else firstMs = now - start;
    last = now;
    frames += (wav.length - data - 8) / 8;
    hash.update(wav.subarray(data + 8));
  }
  const result = await exited;
  if (result !== 0 || !frames) {
    const fault = await readFile(join(folder, 'fault.txt'), 'utf8').catch(() => '');
    throw Error('Renderer stopped after ' + frames + ' frames (' + result + '): ' + fault);
  }
  const elapsedMs = performance.now() - start;
  const speed = frames / 48 / elapsedMs;
  console.log(JSON.stringify({ song, from, beats, frames, elapsedMs, firstMs, worstGapMs, speed, pcmSHA256: hash.digest('hex') }));
  if (speed < 1.5) throw Error('Live throughput is below 1.5× real time.');
} finally {
  await rm(folder, { recursive: true, force: true });
}
