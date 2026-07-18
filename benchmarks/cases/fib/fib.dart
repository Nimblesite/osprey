// Naive recursive Fibonacci — exercises raw function-call + recursion overhead.
int add(int a, int b) => a + b;
int sub(int a, int b) => a - b;

int fib(int n) {
  switch (n) {
    case 0:
      return 0;
    case 1:
      return 1;
    default:
      return add(fib(sub(n, 1)), fib(sub(n, 2)));
  }
}

void main() {
  print(fib(35));
}
