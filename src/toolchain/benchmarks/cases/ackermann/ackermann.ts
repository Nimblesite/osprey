// Ackermann–Péter function — deep, non-tail mutual self-recursion.

function add(a: number, b: number): number {
  return a + b;
}

function sub(a: number, b: number): number {
  return a - b;
}

function ack(m: number, n: number): number {
  if (m === 0) return add(n, 1);
  if (n === 0) return ack(sub(m, 1), 1);
  return ack(sub(m, 1), ack(m, sub(n, 1)));
}

function main(): void {
  console.log(ack(3, 10));
}

main();
