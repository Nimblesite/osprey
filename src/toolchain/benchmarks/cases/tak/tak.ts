// Tak function — heavily recursive integer benchmark.
function sub(a: number, b: number): number {
  return a - b;
}

function tak(x: number, y: number, z: number): number {
  if (x > y) {
    return tak(
      tak(sub(x, 1), y, z),
      tak(sub(y, 1), z, x),
      tak(sub(z, 1), x, y),
    );
  }
  return z;
}

function main(): void {
  console.log(tak(32, 16, 8));
}

main();
