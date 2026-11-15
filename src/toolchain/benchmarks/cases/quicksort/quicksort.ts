// Quicksort — naive first-element-pivot quicksort: split the tail into two
// fresh lists around the pivot, sort both, join. Checksum is rank-weighted.

import { readSeed, sortedChecksum } from "../../shared/seed.ts";

function quicksort(xs: number[]): number[] {
  if (xs.length < 2) {
    return xs;
  }
  const pivot: number = xs[0];
  const below: number[] = [];
  const atLeast: number[] = [];
  for (let i = 1; i < xs.length; i++) {
    if (xs[i] < pivot) {
      below.push(xs[i]);
    } else {
      atLeast.push(xs[i]);
    }
  }
  return [...quicksort(below), pivot, ...quicksort(atLeast)];
}

function main(): void {
  console.log(sortedChecksum(readSeed(), quicksort));
}

main();
