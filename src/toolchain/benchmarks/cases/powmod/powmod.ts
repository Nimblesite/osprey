// Sum of modular exponentiation: sum of i^20 mod P for i in 1..N, naive repeated multiply.
const P: number = 1000000007;

function powmod(base: number, e: number, acc: number): number {
  if (e === 0) {
    return acc;
  }
  return powmod(base, e - 1, (acc * base) % P);
}

function main(): void {
  let acc = 0;
  for (let i = 1; i < 1000000; i++) {
    acc = (acc + powmod(i, 20, 1)) % P;
  }
  console.log(acc);
}

main();
