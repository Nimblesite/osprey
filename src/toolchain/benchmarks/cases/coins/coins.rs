fn coin(k: i64) -> i64 {
    match k {
        1 => 1,
        2 => 5,
        3 => 10,
        4 => 25,
        _ => 50,
    }
}

fn ways(amount: i64, kind: i64) -> i64 {
    if amount == 0 {
        1
    } else if amount < 0 {
        0
    } else if kind == 0 {
        0
    } else {
        ways(amount - coin(kind), kind) + ways(amount, kind - 1)
    }
}

fn main() {
    println!("{}", ways(600, 5));
}
