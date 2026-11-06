// Josephus problem — survivor index for n people, step k=7, via the modular recurrence.
fn main() {
    let mut acc: i64 = 0;
    let mut i: i64 = 2;
    while i < 10000001 {
        acc = (acc + 7) % i;
        i += 1;
    }
    println!("{}", acc);
}
