// Factorial-style product 1*2*...*N taken mod 1000000007 (matches factorial.osp).
const mod = 1000000007n;

function main(): void {
  // acc * i can exceed 2^53, so keep acc in bigint.
  let acc = 1n;
  for (let i = 1; i <= 10000000; i++) {
    acc = (acc * BigInt(i)) % mod;
  }
  console.log(acc.toString());
}

main();
