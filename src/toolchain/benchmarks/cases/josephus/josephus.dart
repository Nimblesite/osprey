// Josephus problem — survivor index for n people, step k=7, via the modular recurrence.
void main() {
  int acc = 0;
  for (int i = 2; i < 10000001; i++) {
    acc = (acc + 7) % i;
  }
  print(acc);
}
