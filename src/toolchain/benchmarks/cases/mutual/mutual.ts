// Mutual recursion — is_even/is_odd counting evens over a loop.
function isEven(n: number): boolean {
  if (n === 0) {
    return true;
  }
  return isOdd(n - 1);
}

function isOdd(n: number): boolean {
  if (n === 0) {
    return false;
  }
  return isEven(n - 1);
}

function main(): void {
  let acc = 0;
  for (let i = 1; i < 130000; i++) {
    if (isEven(i % 1000)) {
      acc += 1;
    }
  }
  console.log(acc);
}

main();
