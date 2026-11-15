// Sum of gcd(i, K) for i in 1..N — Euclidean-algorithm recursion (modulo heavy).
function gcd(a: number, b: number): number {
  return b === 0 ? a : gcd(b, a % b);
}

function main(): void {
  let acc = 0;
  for (let i = 1; i < 2000000; i++) {
    acc += gcd(i, 1234567);
  }
  console.log(acc);
}

main();
