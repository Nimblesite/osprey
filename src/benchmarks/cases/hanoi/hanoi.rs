fn hanoi(n: i64, acc: i64) -> i64 {
    if n == 0 {
        acc
    } else {
        hanoi(n - 1, hanoi(n - 1, acc) + 1)
    }
}

fn main() {
    println!("{}", hanoi(25, 0));
}
