// Triple-nested counting loop accumulating (i*j*k) mod P — nested iteration.
const p: number = 1000000007;

function loopK(i: number, j: number, k: number, acc: number): number {
  while (k !== 0) {
    acc = (acc + i * j * k) % p;
    k -= 1;
  }
  return acc;
}

function loopJ(i: number, j: number, n: number, acc: number): number {
  while (j !== 0) {
    acc = loopK(i, j, n, acc);
    j -= 1;
  }
  return acc;
}

function loopI(i: number, n: number, acc: number): number {
  while (i !== 0) {
    acc = loopJ(i, n, n, acc);
    i -= 1;
  }
  return acc;
}

function main(): void {
  console.log(loopI(250, 250, 0));
}

main();
