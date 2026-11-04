fn is_even(n: i64) -> bool {
    if n == 0 {
        true
    } else {
        is_odd(n - 1)
    }
}

fn is_odd(n: i64) -> bool {
    if n == 0 {
        false
    } else {
        is_even(n - 1)
    }
}

fn main() {
    let mut acc: i64 = 0;
    let mut i: i64 = 1;
    while i < 130000 {
        if is_even(i % 1000) {
            acc += 1;
        }
        i += 1;
    }
    println!("{}", acc);
}
