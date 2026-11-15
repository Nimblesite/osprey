// Text statistics over a tiny vocab — exercises length/char-search per word.

import { hashAt, readSeed } from "../../shared/seed.ts";

function containsChar(w: string, code: number): boolean {
  for (let j = 0; j < w.length; j++) {
    if (w.charCodeAt(j) === code) {
      return true;
    }
  }
  return false;
}

function main(): void {
  const vocab = ["the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog"];
  const seed: number = readSeed();

  let acc = 0;
  for (let i = 0; i < 200000; i++) {
    const w: string = vocab[hashAt(seed, i) % 8];
    acc += w.length +
      (containsChar(w, 0x6f) ? 7 : 0) +
      (w.charCodeAt(0) === 0x74 ? 3 : 0);
  }

  console.log(acc);
}

main();
