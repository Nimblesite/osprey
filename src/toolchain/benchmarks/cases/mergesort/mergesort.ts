// Mergesort — naive top-down merge sort: deal the list into two fresh halves
// by alternating elements, sort both, merge. Checksum is rank-weighted.

import { readSeed, sortedChecksum } from "../../shared/seed.ts";

function merge(left: number[], right: number[]): number[] {
  const merged: number[] = [];
  let i = 0;
  let j = 0;
  while (i < left.length && j < right.length) {
    if (right[j] < left[i]) {
      merged.push(right[j++]);
    } else {
      merged.push(left[i++]);
    }
  }
  while (i < left.length) {
    merged.push(left[i++]);
  }
  while (j < right.length) {
    merged.push(right[j++]);
  }
  return merged;
}

function mergeSort(xs: number[]): number[] {
  if (xs.length < 2) {
    return xs;
  }
  const evens: number[] = [];
  const odds: number[] = [];
  for (let i = 0; i < xs.length; i++) {
    if (i % 2 === 0) {
      evens.push(xs[i]);
    } else {
      odds.push(xs[i]);
    }
  }
  return merge(mergeSort(evens), mergeSort(odds));
}

function main(): void {
  console.log(sortedChecksum(readSeed(), mergeSort));
}

main();
