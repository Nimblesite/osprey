// Ackermann–Péter function — deep, non-tail mutual self-recursion.
fn add(a: i64, b: i64) -> i64 {
    a + b
}

fn sub(a: i64, b: i64) -> i64 {
    a - b
}

fn ack(m: i64, n: i64) -> i64 {
    if m == 0 {
        add(n, 1)
    } else if n == 0 {
        ack(sub(m, 1), 1)
    } else {
        ack(sub(m, 1), ack(m, sub(n, 1)))
    }
}

fn main() {
    println!("{}", ack(3, 10));
}
