// Sum of decimal digit-sums over 1..N — integer division (n/10) and modulo in recursion.

function digsum(n: number): number {
  return n < 10 ? n : (n % 10) + digsum(Math.trunc(n / 10));
}

function main(): void {
  let acc = 0;
  for (let i = 1; i < 2000001; i++) {
    acc += digsum(i);
  }
  console.log(acc);
}

main();
