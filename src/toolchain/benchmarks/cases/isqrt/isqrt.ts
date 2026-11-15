// Sum of integer square roots over 1..N — Newton's method, integer division heavy.
function isqrt(n: number): number {
  if (n < 2) {
    return n;
  }
  let x = n;
  while (true) {
    const y: number = Math.trunc((x + Math.trunc(n / x)) / 2);
    if (y < x) {
      x = y;
    } else {
      return x;
    }
  }
}

function main(): void {
  let acc = 0;
  for (let i = 1; i < 1000001; i++) {
    acc += isqrt(i);
  }
  console.log(acc);
}

main();
