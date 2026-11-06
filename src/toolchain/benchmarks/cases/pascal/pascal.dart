// Binomial coefficient via naive (un-memoised) Pascal recursion C(n,k)=C(n-1,k-1)+C(n-1,k).
int binom(int n, int k) {
  if (k == 0) {
    return 1;
  } else if (k == n) {
    return 1;
  } else {
    return binom(n - 1, k - 1) + binom(n - 1, k);
  }
}

void main() {
  print(binom(27, 13));
}
