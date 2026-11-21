// Phone screenshots against committed goldens ([NATIVE-HOSTS-MOBILE], AGENTS.md § Apps).
//
// One comparison for every phone test: a picture is decoded to RGB with nothing but Node, so every
// runner can compare it with no image tool installed; the system bars are cropped away so the clock
// and signal icons never count, and a pixel differs when any channel moves by more than 24. A picture
// passes when at most 0.5% of its pixels differ and no 64-pixel tile has more than 2% changed, so one
// moved control fails even on a large screen.
//
//   node scripts/mobile/goldens.mjs ACTUAL GOLDEN_FOLDER [--name STEP] [--top FRACTION] [--bottom FRACTION]
//
// The golden is `<width>-<step>.png` in GOLDEN_FOLDER (`ready` when no step is named), so a phone of
// another width fails as missing rather than as different. OSPREY_UPDATE_GOLDENS=1 writes it from
// ACTUAL for review.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { crc32, deflateSync, inflateSync } from "node:zlib";
import { fileURLToPath } from "node:url";

export const CHANNEL_STEP = 24;
export const WHOLE_LIMIT = 0.005;
export const TILE_LIMIT = 0.02;
export const TILE = 64;
export const SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
// 8-bit truecolour, without and with alpha: what phones and simulators write.
const CHANNELS = new Map([[2, 3], [6, 4]]);
// An 8K RGBA screen fits, and malformed dimensions cannot allocate arbitrary scanline storage.
const MAX_SCANLINE_BYTES = 128 * 1024 * 1024;

/** A refusal the command line reports as `FAIL:`. */
export class GoldenProblem extends Error {}

function check(condition, why) {
  if (!condition) throw new GoldenProblem(why);
}

/** Each { kind, body } of a PNG's chunks, in order, every checksum checked. */
export function chunks(data) {
  const found = [];
  let at = SIGNATURE.length;
  while (at < data.length) {
    check(at + 12 <= data.length, "Truncated PNG chunk");
    const length = data.readUInt32BE(at);
    check(length <= data.length - at - 12, "Truncated PNG chunk body");
    const typed = data.subarray(at + 4, at + 8 + length);
    check(data.readUInt32BE(at + 8 + length) === crc32(typed), "PNG chunk checksum mismatch");
    const kind = typed.subarray(0, 4).toString("latin1");
    found.push({ kind, body: typed.subarray(4) });
    at += 12 + length;
    if (kind === "IEND") {
      check(length === 0 && at === data.length, "Invalid PNG end or trailing data");
      return found;
    }
  }
  throw new GoldenProblem("Missing PNG end chunk");
}

/** { width, height, channels } of an 8-bit, non-interlaced truecolour PNG. */
export function header(file, data) {
  check(data.subarray(0, 8).equals(SIGNATURE), `${file} is not a PNG`);
  check(data.length >= 33 && data.subarray(8, 16).equals(Buffer.from("\0\0\0\rIHDR", "latin1")), `${file}: invalid PNG header`);
  const width = data.readUInt32BE(16);
  const height = data.readUInt32BE(20);
  const [depth, colour, compression, filtering, interlace] = data.subarray(24, 29);
  check(width > 0 && height > 0 && compression === 0 && filtering === 0, `${file}: invalid PNG dimensions or methods`);
  check(depth === 8 && CHANNELS.has(colour) && interlace === 0, `${file}: unsupported PNG (depth ${depth}, colour ${colour}, interlace ${interlace})`);
  return { width, height, channels: CHANNELS.get(colour) };
}

function paeth(left, up, corner) {
  const estimate = left + up - corner;
  const nearLeft = Math.abs(estimate - left);
  const nearUp = Math.abs(estimate - up);
  const nearCorner = Math.abs(estimate - corner);
  if (nearLeft <= nearUp && nearLeft <= nearCorner) return left;
  return nearUp <= nearCorner ? up : corner;
}

function predicted(kind, left, up, corner) {
  if (kind === 1) return left;
  if (kind === 2) return up;
  if (kind === 3) return Math.floor((left + up) / 2);
  return kind === 4 ? paeth(left, up, corner) : 0;
}

/** One scanline with its PNG filter undone in place. */
export function unfilter(kind, line, previous, step) {
  check(kind >= 0 && kind <= 4, `Unknown PNG scanline filter ${kind}`);
  for (let i = 0; i < line.length; i += 1) {
    const left = i >= step ? line[i - step] : 0;
    const corner = i >= step ? previous[i - step] : 0;
    line[i] = (line[i] + predicted(kind, left, previous[i], corner)) & 255;
  }
  return line;
}

function inflated(file, data, expected) {
  const compressed = Buffer.concat(chunks(data).filter((chunk) => chunk.kind === "IDAT").map((chunk) => chunk.body));
  try {
    return inflateSync(compressed, { maxOutputLength: expected + 1 });
  } catch {
    throw new GoldenProblem(`${file}: invalid PNG compressed data`);
  }
}

/** The PNG bytes `data` as { width, height, pixels } with rgb24 pixels. */
export function decode(file, data) {
  const { width, height, channels } = header(file, data);
  const stride = width * channels;
  const expected = height * (stride + 1);
  check(expected <= MAX_SCANLINE_BYTES, `${file}: PNG exceeds screenshot decoding capacity`);
  const raw = inflated(file, data, expected);
  check(raw.length === expected, `${file}: invalid PNG scanline size`);
  const pixels = Buffer.alloc(width * height * 3);
  let previous = Buffer.alloc(stride);
  for (let row = 0; row < height; row += 1) {
    const start = row * (stride + 1);
    previous = unfilter(raw[start], Buffer.from(raw.subarray(start + 1, start + 1 + stride)), previous, channels);
    for (let x = 0; x < width; x += 1) previous.copy(pixels, (row * width + x) * 3, x * channels, x * channels + 3);
  }
  return { width, height, pixels };
}

/** The picture at `file` as { width, height, pixels }. */
export function pngPixels(file) {
  return decode(file, readFileSync(file));
}

function chunk(kind, body) {
  const typed = Buffer.concat([Buffer.from(kind, "latin1"), body]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(body.length);
  const sum = Buffer.alloc(4);
  sum.writeUInt32BE(crc32(typed));
  return Buffer.concat([length, typed, sum]);
}

/** 8-bit truecolour PNG bytes for `pixels` (rgb24, or rgba with `alpha`), unfiltered. */
export function pngEncode(width, height, pixels, alpha = false) {
  const stride = width * (alpha ? 4 : 3);
  check(width > 0 && height > 0 && pixels.length === stride * height, "Invalid PNG dimensions or pixel count");
  const rows = Buffer.alloc(height * (stride + 1));
  for (let row = 0; row < height; row += 1) pixels.copy(rows, row * (stride + 1) + 1, row * stride, (row + 1) * stride);
  const head = Buffer.alloc(13);
  head.writeUInt32BE(width, 0);
  head.writeUInt32BE(height, 4);
  head.set([8, alpha ? 6 : 2, 0, 0, 0], 8);
  return Buffer.concat([SIGNATURE, chunk("IHDR", head), chunk("IDAT", deflateSync(rows, { level: 6 })), chunk("IEND", Buffer.alloc(0))]);
}

function changes(actual, expected) {
  check(actual.width === expected.width && actual.height === expected.height, `the picture is ${actual.width}×${actual.height}, the golden ${expected.width}×${expected.height}`);
  const { width, height } = actual;
  check(actual.pixels.length === width * height * 3 && expected.pixels.length === width * height * 3, "Invalid screenshot pixel count");
  const changed = new Uint8Array(width * height);
  for (let i = 0; i < changed.length; i += 1) {
    const at = i * 3;
    changed[i] = [0, 1, 2].some((c) => Math.abs(actual.pixels[at + c] - expected.pixels[at + c]) > CHANNEL_STEP) ? 1 : 0;
  }
  return changed;
}

function region(changed, width, height, x, y, tile) {
  const right = Math.min(width, x + tile);
  const bottom = Math.min(height, y + tile);
  let count = 0;
  for (let row = y; row < bottom; row += 1) for (let column = x; column < right; column += 1) count += changed[row * width + column];
  return count / ((right - x) * (bottom - y));
}

/** The share of pixels that differ, or with `tile` the worst share in any tile of that size. */
export function pictureDelta(actual, expected, tile = 0) {
  const changed = changes(actual, expected);
  const { width, height } = actual;
  if (!tile) return changed.reduce((sum, one) => sum + one, 0) / (width * height);
  let worst = 0;
  for (let y = 0; y < height; y += tile) for (let x = 0; x < width; x += tile) worst = Math.max(worst, region(changed, width, height, x, y, tile));
  return worst;
}

/** `picture` without the given fractions of its height at the top and the bottom. */
export function cropped(picture, top, bottom) {
  const { width, height, pixels } = picture;
  const first = Math.floor(height * top);
  const last = height - Math.floor(height * bottom);
  return { width, height: last - first, pixels: pixels.subarray(first * width * 3, last * width * 3) };
}

/** Compares the cropped picture with its golden, writing the crop beside the picture for review. */
export function verify(actualPath, folder, { name = "ready", top = 0.08, bottom = 0.05 } = {}) {
  const actual = cropped(pngPixels(actualPath), top, bottom);
  const parsed = path.parse(actualPath);
  const crop = path.join(parsed.dir, `${parsed.name}-content.png`);
  writeFileSync(crop, pngEncode(actual.width, actual.height, actual.pixels));
  const golden = path.join(folder, `${actual.width}-${name}.png`);
  if (process.env.OSPREY_UPDATE_GOLDENS === "1") {
    mkdirSync(folder, { recursive: true });
    writeFileSync(golden, readFileSync(crop));
  }
  check(existsSync(golden), `Missing ${golden}; rerun with OSPREY_UPDATE_GOLDENS=1 and review the image`);
  const expected = pngPixels(golden);
  const whole = pictureDelta(actual, expected);
  const worst = pictureDelta(actual, expected, TILE);
  const percent = (share, digits) => `${(share * 100).toFixed(digits)}%`;
  check(whole <= WHOLE_LIMIT && worst <= TILE_LIMIT, `${percent(whole, 2)} of pixels differ, worst tile ${percent(worst, 2)}; inspect ${crop} against ${golden}`);
  return `PASS: ${path.relative(process.cwd(), golden)} (${percent(whole, 3)} differing pixels)`;
}

function option(argv, flag, fallback) {
  const at = argv.indexOf(flag);
  return at >= 0 && at + 1 < argv.length ? argv[at + 1] : fallback;
}

/** The command line: exit 0 with `PASS:`, or 1 with `FAIL:` and the reason. */
export function main(argv) {
  const [actual, folder] = argv;
  try {
    check(actual !== undefined && folder !== undefined, "usage: goldens.mjs ACTUAL GOLDEN_FOLDER [--name STEP] [--top FRACTION] [--bottom FRACTION]");
    console.log(verify(actual, folder, { name: option(argv, "--name", "ready"), top: Number(option(argv, "--top", "0.08")), bottom: Number(option(argv, "--bottom", "0.05")) }));
    return 0;
  } catch (problem) {
    console.error(`FAIL: ${problem instanceof Error ? problem.message : String(problem)}`);
    return 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exit(main(process.argv.slice(2)));
