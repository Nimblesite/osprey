// Word-frequency count over a tiny vocab — exercises hashing + counting.

import { hashAt, readSeed } from "../../shared/seed.ts";

function main(): void {
  const seed: number = readSeed();

  const counts: number[] = new Array<number>(8).fill(0);
  for (let i = 0; i < 200000; i++) {
    counts[hashAt(seed, i) % 8] += 1;
  }

  let result = 0;
  for (let k = 0; k < 8; k++) {
    result += (k + 1) * counts[k];
  }

  console.log(result);
}

main();
