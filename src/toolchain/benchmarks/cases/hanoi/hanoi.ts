// Towers of Hanoi move count via double recursion with an accumulator.
function hanoi(n: number, acc: number): number {
  if (n === 0) {
    return acc;
  }
  return hanoi(n - 1, hanoi(n - 1, acc) + 1);
}

function main(): void {
  console.log(hanoi(25, 0));
}

main();
