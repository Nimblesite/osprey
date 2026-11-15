// The seed and hash the list and text cases share, imported by each case so
// one case file runs standalone with `node cases/<name>/<name>.ts`.
import { readFileSync } from "node:fs";
import { randomBytes } from "node:crypto";

const minstd: number = 16807;
const modulus: number = 2147483647;

function r1(x: number): number {
  return (x * minstd) % modulus;
}

export function hashAt(s: number, i: number): number {
  return r1(r1(s + i));
}

// Reads the seed's leading integer as C's strtoll does: 0 selects the fixed
// seed 1, anything else a random one.
export function readSeed(): number {
  const all: string = readFileSync(0, "utf8");
  const nl: number = all.indexOf("\n");
  const line: string = nl === -1 ? all : all.slice(0, nl);
  let m = 0;
  let end = 0;
  while (end < line.length && (line.charCodeAt(end) === 0x20 || line.charCodeAt(end) === 0x09)) {
    end++;
  }
  const start = end;
  if (end < line.length && (line.charCodeAt(end) === 0x2b || line.charCodeAt(end) === 0x2d)) {
    end++;
  }
  while (end < line.length && line.charCodeAt(end) >= 0x30 && line.charCodeAt(end) <= 0x39) {
    end++;
  }
  if (end > start) {
    const parsed: number = Number(line.substring(start, end));
    if (Number.isSafeInteger(parsed)) {
      m = parsed;
    }
  }
  if (m === 0) {
    return 1;
  }
  // A 64-bit random value masked to 63 bits, reduced to [1, 2147483646].
  const hi = BigInt(randomBytes(4).readUInt32BE(0));
  const lo = BigInt(randomBytes(4).readUInt32BE(0));
  const val = ((hi << 32n) | lo) & ((1n << 63n) - 1n);
  return Number(val % 2147483646n) + 1;
}

// The sort cases' shared run: eight seeded runs of 2000 values below 100000,
// each sorted by `sort` and folded into one rank-weighted checksum.
export function sortedChecksum(seed: number, sort: (xs: number[]) => number[]): number {
  const bigMod: number = 1000000007;
  let acc = 0;
  for (let t = 0; t < 8; t++) {
    const s: number = seed + t * 131;
    const sorted: number[] = sort(Array.from({ length: 2000 }, (_v, i) => hashAt(s, i) % 100000));
    let weight = 0;
    for (let i = 0; i < sorted.length; i++) {
      weight = (weight + sorted[i] * (i + 1)) % bigMod;
    }
    acc = (acc + weight) % bigMod;
  }
  return acc;
}
