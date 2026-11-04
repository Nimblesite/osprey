//! [MEM-EFFECT-BATON] A handled computation runs on its own thread, passing one
//! baton with its host. The collector keeps tracing across that handoff, so a
//! handled program can't grow without bound, and values only the runtime or a
//! parked stack can see survive every collection.
use std::process::Command;

const PROGRAM: &str = r#"effect Box {
    control swap: fn(string) -> string
    control measure: fn(List<int>) -> int
}

fn churn(rounds: int) = fold(range(0, rounds), [], fn(acc, _i) => listConcat(acc, [195, 169]))

fn digits(n: int) -> int = match n <= 0 {
    true => 0
    false => satAdd(1, digits(satSub(n, 1)))
}

fn exchange(acc: int, i: int) -> int = {
    let held = "held-${i}"
    let sent = perform Box.swap("v${i}")
    let measured = perform Box.measure(churn(40))
    let kept = churn(400)
    match sent == "seen v${i}" && held == "held-${i}" && measured == 80 && listLength(kept) == 800 {
        true => satAdd(acc, 1)
        false => acc
    }
}

fn body() = {
    let intact = fold(range(0, 300), 0, exchange)
    print("intact ${intact}")
    print("deep ${digits(200000)}")
    print("built ${listLength(churn(12000))}")
}

fn main() = {
    handle Box {
        swap text => {
            let noise = churn(300)
            resume("seen ${text}${match listLength(noise) == 600 { true => "" false => "!" }}")
        }
        measure items => {
            let noise = churn(300)
            resume(satAdd(listLength(items), satSub(listLength(noise), 600)))
        }
    }
    body()
}
"#;

const EXPECTED: &str = "intact 300\ndeep 200000\nbuilt 24000\n";

fn run(memory: &str) -> (String, String, Option<i32>) {
    let source = std::env::temp_dir().join(format!(
        "osprey_gc_baton_{}_{memory}.osp",
        std::process::id()
    ));
    assert!(std::fs::write(&source, PROGRAM).is_ok(), "not written");
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&source)
        .arg(format!("--memory={memory}"))
        .args(["--run", "--quiet", "--no-warnings"])
        .env("OSPREY_GC_DEBUG", "1")
        .output();
    let _ = std::fs::remove_file(&source);
    assert!(output.is_ok(), "{memory}: not run");
    let output = output.unwrap_or_else(|_| unreachable!());
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code(),
    )
}

#[test]
fn the_collector_keeps_tracing_inside_a_resumable_handler() {
    let (stdout, stderr, code) = run("gc");
    assert_eq!((code, stdout.as_str()), (Some(0), EXPECTED), "{stderr}");
    let collections = stderr.matches("[osp-gc] collect #").count();
    assert!(collections > 100, "only {collections} collections");
    assert!(!stderr.contains("collection disabled"), "{stderr}");
    // Churn stays garbage: nothing near the 24,000-element list's quadratic total.
    let peak = stderr
        .lines()
        .filter_map(|line| line.split(" objs, ").nth(1))
        .filter_map(|tail| tail.split(' ').next())
        .filter_map(|kib| kib.parse::<u64>().ok())
        .max()
        .unwrap_or(u64::MAX);
    assert!(peak < 64 * 1024, "live set reached {peak} KiB");
}

#[test]
fn the_same_handled_program_is_exact_under_arc() {
    let (stdout, stderr, code) = run("arc");
    assert_eq!((code, stdout.as_str()), (Some(0), EXPECTED), "{stderr}");
}
