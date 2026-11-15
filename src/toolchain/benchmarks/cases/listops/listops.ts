// List operations — build arrays, then sum + count below threshold.

import { hashAt, readSeed } from "../../shared/seed.ts";

const bigMod: number = 1000000007;
const n: number = 4000;

function main(): void {
  const seed: number = readSeed();

  let acc = 0;
  for (let t = 0; t < 8; t++) {
    const s: number = seed + t * 131;
    const xs: number[] = new Array<number>(n).fill(0);
    for (let i = 0; i < n; i++) {
      xs[i] = hashAt(s, i) % 1000;
    }
    let sum = 0;
    let below = 0;
    for (let i = 0; i < n; i++) {
      sum += xs[i];
      if (xs[i] < 500) {
        below++;
      }
    }
    acc = (acc + sum + below) % bigMod;
  }

  console.log(acc);
}

main();
