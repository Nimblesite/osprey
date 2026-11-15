// Naive recursive Fibonacci — exercises raw function-call + recursion overhead.
function add(a: number, b: number): number {
  return a + b;
}

function sub(a: number, b: number): number {
  return a - b;
}

function fib(n: number): number {
  switch (n) {
    case 0:
      return 0;
    case 1:
      return 1;
    default:
      return add(fib(sub(n, 1)), fib(sub(n, 2)));
  }
}

function main(): void {
  console.log(fib(35));
}

main();
