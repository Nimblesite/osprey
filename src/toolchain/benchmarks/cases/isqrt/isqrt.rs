// Sum of integer square roots over 1..N — Newton's method, integer division heavy.
fn isqrt(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    loop {
        let y = (x + n / x) / 2;
        if y < x {
            x = y;
        } else {
            return x;
        }
    }
}

fn main() {
    let mut acc: i64 = 0;
    let mut i: i64 = 1;
    while i < 1000001 {
        acc += isqrt(i);
        i += 1;
    }
    println!("{}", acc);
}
