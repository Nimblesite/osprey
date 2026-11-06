// Sum of decimal digit-sums over 1..N — integer division (n/10) and modulo in recursion.
fn digsum(n: i64) -> i64 {
    if n < 10 {
        n
    } else {
        n % 10 + digsum(n / 10)
    }
}

fn main() {
    let mut acc: i64 = 0;
    let mut i: i64 = 1;
    while i < 2000001 {
        acc += digsum(i);
        i += 1;
    }
    println!("{}", acc);
}
