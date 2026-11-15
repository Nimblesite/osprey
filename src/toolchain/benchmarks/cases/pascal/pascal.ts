// Binomial coefficient via naive (un-memoised) Pascal recursion C(n,k)=C(n-1,k-1)+C(n-1,k).
function binom(n: number, k: number): number {
  if (k === 0) {
    return 1;
  } else if (k === n) {
    return 1;
  } else {
    return binom(n - 1, k - 1) + binom(n - 1, k);
  }
}

function main(): void {
  console.log(binom(27, 13));
}

main();
