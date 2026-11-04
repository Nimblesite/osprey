// Count primes below a limit by trial division — integer % in a tight loop.
fn has_factor(n: i64, d: i64) -> bool {
    if d * d > n {
        false
    } else if n % d == 0 {
        true
    } else {
        has_factor(n, d + 1)
    }
}

fn is_prime(n: i64) -> bool {
    if n < 2 {
        false
    } else {
        !has_factor(n, 2)
    }
}

fn main() {
    let mut acc: i64 = 0;
    let mut n: i64 = 2;
    while n < 200000 {
        if is_prime(n) {
            acc += 1;
        }
        n += 1;
    }
    println!("{acc}");
}
