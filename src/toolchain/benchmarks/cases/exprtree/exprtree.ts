// Expression tree — build a heap-allocated tree, then evaluate it.
import { readFileSync } from "node:fs";
import { randomBytes } from "node:crypto";

import { hashAt } from "../../shared/seed.ts";

const bigMod: number = 1000000007;

const tagLit: number = 0;
const tagAdd: number = 1;
const tagMul: number = 2;

function readSeed(): number {
  const all: string = readFileSync(0, "utf8");
  const nl: number = all.indexOf("\n");
  const line: string = nl === -1 ? all : all.slice(0, nl);
  const parsed: number = Number(line.trim());
  const m: number = Number.isFinite(parsed) ? parsed : 0;
  if (m === 0) {
    return 1;
  }
  return Number(BigInt("0x" + randomBytes(4).toString("hex")) % 2147483646n) + 1;
}

class Expr {
  tag: number = tagLit;
  v: number = 0;
  l: Expr | null = null;
  r: Expr | null = null;
}

function build(s: number, idx: number, depth: number): Expr {
  const e = new Expr();
  if (depth === 0) {
    e.tag = tagLit;
    e.v = hashAt(s, idx) % 1000;
  } else {
    const op: number = hashAt(s, idx) % 2;
    e.v = 0;
    e.l = build(s, idx * 2 + 1, depth - 1);
    e.r = build(s, idx * 2 + 2, depth - 1);
    e.tag = op === 0 ? tagAdd : tagMul;
  }
  return e;
}

// Renamed from Dart's `eval`, which is a reserved binding in strict-mode JS.
function evaluate(e: Expr): number {
  switch (e.tag) {
    case tagLit:
      return e.v;
    case tagAdd:
      return (evaluate(e.l!) + evaluate(e.r!)) % bigMod;
    default:
      // The product of two values near 1e9 exceeds 2^53, so multiply in bigint.
      return Number((BigInt(evaluate(e.l!)) * BigInt(evaluate(e.r!))) % BigInt(bigMod));
  }
}

function main(): void {
  const seed: number = readSeed();

  let acc = 0;
  for (let t = 0; t < 10; t++) {
    acc = (acc + evaluate(build(seed + t * 7, 1, 14))) % bigMod;
  }

  console.log(acc);
}

main();
