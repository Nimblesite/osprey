// Ackermann–Péter function — deep, non-tail mutual self-recursion.

int add(int a, int b) => a + b;
int sub(int a, int b) => a - b;

int ack(int m, int n) {
  if (m == 0) return add(n, 1);
  if (n == 0) return ack(sub(m, 1), 1);
  return ack(sub(m, 1), ack(m, sub(n, 1)));
}

void main() {
  print(ack(3, 10));
}
