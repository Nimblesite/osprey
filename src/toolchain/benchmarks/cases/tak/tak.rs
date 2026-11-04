fn sub(a: i64, b: i64) -> i64 {
    a - b
}

fn tak(x: i64, y: i64, z: i64) -> i64 {
    if x > y {
        tak(
            tak(sub(x, 1), y, z),
            tak(sub(y, 1), z, x),
            tak(sub(z, 1), x, y),
        )
    } else {
        z
    }
}

fn main() {
    println!("{}", tak(32, 16, 8));
}
