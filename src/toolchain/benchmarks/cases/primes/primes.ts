// Count primes below a limit by trial division — integer % in a tight loop.
function hasFactor(n: number, d: number): boolean {
  if (d * d > n) {
    return false;
  } else if (n % d === 0) {
    return true;
  } else {
    return hasFactor(n, d + 1);
  }
}

function isPrime(n: number): boolean {
  if (n < 2) {
    return false;
  }
  return !hasFactor(n, 2);
}

function main(): void {
  let acc = 0;
  for (let n = 2; n < 200000; n++) {
    if (isPrime(n)) {
      acc += 1;
    }
  }
  console.log(acc);
}

main();
