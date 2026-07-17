// Tak function — heavily recursive integer benchmark.
int sub(int a, int b) {
  return a - b;
}

int tak(int x, int y, int z) {
  if (x > y) {
    return tak(
        tak(sub(x, 1), y, z), tak(sub(y, 1), z, x), tak(sub(z, 1), x, y));
  }
  return z;
}

void main() {
  print(tak(32, 16, 8));
}
