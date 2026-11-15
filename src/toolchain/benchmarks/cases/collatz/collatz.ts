// Collatz (3n+1) stopping time summed over 1..N — integer division (n/2) in deep recursion.

function collatz(n: number): number {
  if (n === 1) return 0;
  return n % 2 === 0 ? 1 + collatz(Math.trunc(n / 2)) : 1 + collatz(3 * n + 1);
}

function main(): void {
  let acc = 0;
  for (let i = 1; i < 100001; i++) {
    acc += collatz(i);
  }
  console.log(acc);
}

main();
