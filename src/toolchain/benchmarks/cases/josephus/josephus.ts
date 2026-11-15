// Josephus problem — survivor index for n people, step k=7, via the modular recurrence.
function main(): void {
  let acc = 0;
  for (let i = 2; i < 10000001; i++) {
    acc = (acc + 7) % i;
  }
  console.log(acc);
}

main();
