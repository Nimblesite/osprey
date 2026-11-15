// Count coprime pairs (i,j), 1<=i,j<=N — nested iteration + Euclidean gcd.

function gcd(a: number, b: number): number {
  return b === 0 ? a : gcd(b, a % b);
}

function main(): void {
  const n = 2000;
  let acc = 0;
  for (let i = n; i > 0; i--) {
    for (let j = n; j > 0; j--) {
      if (gcd(i, j) === 1) {
        acc++;
      }
    }
  }
  console.log(acc);
}

main();
