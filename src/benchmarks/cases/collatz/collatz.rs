// Collatz (3n+1) stopping time summed over 1..N — integer division (n/2) in deep recursion.
fn collatz(n: i64) -> i64 {
    if n == 1 {
        0
    } else if n % 2 == 0 {
        1 + collatz(n / 2)
    } else {
        1 + collatz(3 * n + 1)
    }
}

fn main() {
    let mut acc: i64 = 0;
    let mut i: i64 = 1;
    while i < 100001 {
        acc += collatz(i);
        i += 1;
    }
    println!("{}", acc);
}
