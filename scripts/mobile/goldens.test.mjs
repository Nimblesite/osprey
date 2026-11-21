// [NATIVE-HOSTS-MOBILE] screenshots are compared with nothing but Node, on every runner.
//   node --test scripts/mobile/goldens.test.mjs
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { crc32, deflateSync } from "node:zlib";
import { GoldenProblem, SIGNATURE, decode, pictureDelta, pngEncode, pngPixels } from "./goldens.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const GOLDENS = path.join(here, "goldens.mjs");
const GOLDEN = path.resolve(here, "../../src/osprey/examples/tally/goldens/android/1080-ready.png");

function chunk(kind, body) {
  const typed = Buffer.concat([Buffer.from(kind, "latin1"), body]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(body.length);
  const sum = Buffer.alloc(4);
  sum.writeUInt32BE(crc32(typed));
  return Buffer.concat([length, typed, sum]);
}

function picture(raw, { width = 2, height = 2, compression = 0, filtering = 0 } = {}) {
  const head = Buffer.alloc(13);
  head.writeUInt32BE(width >>> 0, 0);
  head.writeUInt32BE(height >>> 0, 4);
  head.set([8, 2, compression, filtering, 0], 8);
  return Buffer.concat([SIGNATURE, chunk("IHDR", head), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

function compare(actual, folder, extra = []) {
  return spawnSync(process.execPath, [GOLDENS, actual, folder, "--top", "0", "--bottom", "0", ...extra], { encoding: "utf8", env: { PATH: "/usr/bin:/bin" } });
}

const RAW = Buffer.concat([Buffer.from([0]), Buffer.alloc(6), Buffer.from([0]), Buffer.alloc(6)]);

function malformed(valid) {
  return {
    "short row": picture(RAW.subarray(0, RAW.length - 3)),
    "extra bytes": picture(Buffer.concat([RAW, Buffer.from("x")])),
    "unknown filter": picture(Buffer.concat([Buffer.from([5]), RAW.subarray(1)])),
    "empty width": picture(Buffer.alloc(2), { width: 0 }),
    compression: picture(RAW, { compression: 1 }),
    "filter method": picture(RAW, { filtering: 1 }),
    checksum: Buffer.concat([valid.subarray(0, 29), Buffer.from([valid[29] ^ 1]), valid.subarray(30)]),
    "missing end": valid.subarray(0, valid.length - 12),
    "truncated chunk": valid.subarray(0, valid.length - 1),
    "trailing data": Buffer.concat([valid, Buffer.from("x")]),
    "short header": SIGNATURE,
    "oversized dimensions": picture(RAW, { width: 2 ** 31, height: 2 ** 31 }),
    "invalid zlib": Buffer.concat([valid.subarray(0, 33), chunk("IDAT", Buffer.from("not zlib")), chunk("IEND", Buffer.alloc(0))]),
  };
}

test("a malformed PNG can never pass a golden", () => {
  const folder = mkdtempSync(path.join(tmpdir(), "goldens-"));
  const screen = path.join(folder, "screen.png");
  const valid = picture(RAW);
  writeFileSync(path.join(folder, "2-ready.png"), valid);
  writeFileSync(screen, valid);
  assert.equal(compare(screen, folder).status, 0);
  for (const [reason, data] of Object.entries(malformed(valid))) {
    writeFileSync(screen, data);
    assert.throws(() => pngPixels(screen), GoldenProblem, reason);
    const refused = compare(screen, folder);
    assert.equal(refused.status, 1, reason);
    assert.match(refused.stderr, /FAIL:/, reason);
  }
});

test("split IDAT chunks and every scanline filter reconstruct known RGB bytes", () => {
  const compressed = deflateSync(RAW);
  const valid = picture(RAW);
  const split = Buffer.concat([valid.subarray(0, 33), chunk("IDAT", compressed.subarray(0, 5)), chunk("IDAT", compressed.subarray(5)), chunk("IEND", Buffer.alloc(0))]);
  assert.deepEqual(decode("split", split), { width: 2, height: 2, pixels: Buffer.alloc(12) });
  const pixels = Buffer.from([10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120]);
  const residuals = [pixels,
    Buffer.from([10, 20, 30, 30, 30, 30, 70, 80, 90, 30, 30, 30]),
    Buffer.from([10, 20, 30, 40, 50, 60, 60, 60, 60, 60, 60, 60]),
    Buffer.from([10, 20, 30, 35, 40, 45, 65, 70, 75, 45, 45, 45]),
    Buffer.from([10, 20, 30, 30, 30, 30, 60, 60, 60, 30, 30, 30])];
  residuals.forEach((residual, kind) => {
    const raw = Buffer.concat([Buffer.from([kind]), residual.subarray(0, 6), Buffer.from([kind]), residual.subarray(6)]);
    assert.deepEqual(decode(`filter ${kind}`, picture(raw)), { width: 2, height: 2, pixels }, `filter ${kind}`);
  });
  for (const wrong of [Buffer.alloc(9), Buffer.alloc(15)]) {
    assert.throws(() => pngEncode(2, 2, wrong), GoldenProblem);
    assert.throws(() => pictureDelta({ width: 2, height: 2, pixels: wrong }, { width: 2, height: 2, pixels: Buffer.alloc(12) }), GoldenProblem);
  }
});

test("the decoder reads the same pixels ffmpeg does, where ffmpeg is installed", (context) => {
  if (spawnSync("ffmpeg", ["-version"]).status !== 0) {
    context.skip("ffmpeg cross-checks the decoder where it is installed");
    return;
  }
  const { width, height, pixels } = pngPixels(GOLDEN);
  const decoded = execFileSync("ffmpeg", ["-v", "error", "-i", GOLDEN, "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"], { maxBuffer: 1 << 26 });
  assert.deepEqual([width, height], [1080, 2088]);
  assert.ok(pixels.equals(decoded));
});

function rgba(pixels) {
  const out = Buffer.alloc((pixels.length / 3) * 4);
  for (let i = 0; i < pixels.length / 3; i += 1) {
    pixels.copy(out, i * 4, i * 3, i * 3 + 3);
    out[i * 4 + 3] = i % 256;
  }
  return out;
}

function moved(pixels, width) {
  const copy = Buffer.from(pixels);
  for (let row = 400; row < 480; row += 1) {
    const start = (row * width + 200) * 3;
    for (let i = start; i < start + 80 * 3; i += 1) copy[i] = 255 - copy[i];
  }
  return copy;
}

test("screens compare with Node alone, named steps have their own goldens, and a moved control fails", () => {
  const { width, height, pixels } = pngPixels(GOLDEN);
  const root = mkdtempSync(path.join(tmpdir(), "goldens-"));
  copyFileSync(GOLDEN, path.join(root, `${width}-ready.png`));
  copyFileSync(GOLDEN, path.join(root, `${width}-deposit.png`));
  writeFileSync(path.join(root, "same.png"), pngEncode(width, height, rgba(pixels), true));
  const same = compare(path.join(root, "same.png"), root);
  assert.equal(same.status, 0, same.stderr);
  assert.match(same.stdout, /\(0\.000% differing pixels\)/);
  assert.ok(pngPixels(path.join(root, "same-content.png")).pixels.equals(pixels), "the review crop is the screen");
  assert.equal(compare(path.join(root, "same.png"), root, ["--name", "deposit"]).status, 0);
  const missing = compare(path.join(root, "same.png"), root, ["--name", "transfer"]);
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /Missing .*-transfer\.png/);
  writeFileSync(path.join(root, "moved.png"), pngEncode(width, height, moved(pixels, width)));
  const failed = compare(path.join(root, "moved.png"), root);
  assert.equal(failed.status, 1, failed.stdout);
  assert.match(failed.stderr, /FAIL: .*of pixels differ, worst tile/);
  writeFileSync(path.join(root, "text.png"), "not a PNG");
  const refused = compare(path.join(root, "text.png"), root);
  assert.notEqual(refused.status, 0);
  assert.match(refused.stderr, /not a PNG/);
  assert.ok(readFileSync(GOLDEN).length > 0);
});
