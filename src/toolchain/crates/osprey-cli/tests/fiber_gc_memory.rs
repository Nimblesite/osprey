//! Garbage must be reclaimed however a program uses threads: a runaway here costs
//! the whole machine, so every run is killed at its memory budget. [MEM-BACKENDS]
#![cfg(unix)]

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// About 2.5 GB of garbage per `churn()`, none of it retained.
const CHURN: &str = "fn churn() = range(0, 2000000) |> fold(0, fn(sum, i) => wrapAdd(sum, length(listAppend([1, 2, 3, 4, 5, 6, 7, 8], i))))\nprint(length(listAppend([1, 2], 3)))\n";
const MEMORY_BUDGET_MB: u64 = 1024;
const RUN_DEADLINE: Duration = Duration::from_secs(120);

/// Resident memory of every process in `group`, in MiB.
fn group_rss_mb(group: u32) -> io::Result<u64> {
    let listing = Command::new("ps").args(["-axo", "pgid=,rss="]).output()?;
    let text = String::from_utf8_lossy(&listing.stdout).into_owned();
    let kib: u64 = text
        .lines()
        .filter_map(|line| line.trim().split_once(char::is_whitespace))
        .filter(|row| row.0.parse() == Ok(group))
        .filter_map(|row| row.1.trim().parse::<u64>().ok())
        .sum();
    Ok(kib / 1024)
}

/// Runs `source`, killing its process group past the budget so a runaway cannot
/// take the machine down. Returns stdout, or why the run was stopped.
fn run_within_budget(name: &str, source: &str, memory: &str) -> io::Result<Result<String, String>> {
    let file = format!(
        "osprey_fiber_budget_{}_{name}_{memory}.osp",
        std::process::id()
    );
    let path = std::env::temp_dir().join(file);
    std::fs::write(&path, source)?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&path)
        .args(["--run", "--quiet", &format!("--memory={memory}")])
        .stdout(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let started = Instant::now();
    let stopped = loop {
        if child.try_wait()?.is_some() {
            break None;
        }
        let rss = group_rss_mb(child.id())?;
        if rss > MEMORY_BUDGET_MB || started.elapsed() > RUN_DEADLINE {
            let group = format!("-{}", child.id());
            let _ = Command::new("kill").args(["-9", &group]).status()?;
            break Some(format!(
                "stopped at {rss} MiB after {:?}",
                started.elapsed()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let output = child.wait_with_output()?;
    std::fs::remove_file(&path)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(stopped.map_or(Ok(stdout), Err))
}

/// [MEM-BACKENDS] Garbage is reclaimed whichever thread made it. Under GC, the
/// first allocation by a fiber stopped collection for the rest of the process, so
/// a program retaining almost nothing grew until the OS killed other processes.
#[test]
fn garbage_made_with_fibers_is_reclaimed_in_every_backend() -> io::Result<()> {
    let cases = [
        ("no_fiber", "print(churn())\n", "3\n18000000\n"),
        (
            "one_fiber",
            "let f = spawn churn()\nprint(await(f))\n",
            "3\n18000000\n",
        ),
        (
            "two_fibers",
            "let a = spawn churn()\nlet b = spawn churn()\nprint(wrapAdd(await(a), await(b)))\n",
            "3\n36000000\n",
        ),
        (
            "main_after_a_finished_fiber",
            "let f = spawn length(listAppend([1], 2))\nprint(await(f))\nprint(churn())\n",
            "3\n2\n18000000\n",
        ),
    ];
    let mut wrong = Vec::new();
    for (name, body, expected) in cases {
        for memory in ["gc", "arc"] {
            let result = run_within_budget(name, &format!("{CHURN}{body}"), memory)?;
            if result.as_deref() != Ok(expected) {
                wrong.push(format!("{name}/{memory}: {result:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

/// Garbage to force collections, and a sum that reads every element of `items`.
const LIVE_PRELUDE: &str = "fn churn(n) = range(0, n) |> fold(0, fn(sum, i) => wrapAdd(sum, length(listAppend([1, 2, 3, 4, 5, 6, 7, 8], i))))\nfn total(items) = forEachSum(items, 0, 0)\nfn forEachSum(items, index, sum) = match listGet(items, index) {\n  Success { value } => forEachSum(items, wrapAdd(index, 1), wrapAdd(sum, value))\n  _ => sum\n}\nfn made(n) = range(0, n) |> fold([], fn(items, i) => listAppend(items, wrapAdd(i, 1)))\n";

/// [MEM-GC-SAFEPOINT] Collections now run while fibers do, so everything only the
/// runtime or another thread's stack holds must survive them: a wrong answer or a
/// crash here means a live value was freed.
#[test]
fn values_only_the_runtime_holds_survive_collections_during_fibers() -> io::Result<()> {
    let cases = [
        (
            "result_held_by_a_finished_fiber",
            "let f = spawn made(100)\nprint(churn(500000))\nprint(total(await(f)))\n",
            "4500000\n5050\n",
        ),
        (
            "capture_read_after_collections",
            "let kept = made(100)\nlet f = spawn { let c = churn(500000)\n  wrapAdd(c, total(kept)) }\nprint(await(f))\nprint(total(kept))\n",
            "4505050\n5050\n",
        ),
        (
            "values_buffered_in_a_channel",
            "let c = Channel(4)\nsend(c, made(10))\nsend(c, made(20))\nlet f = spawn churn(500000)\nprint(await(f))\nprint(total(recv(c)))\nprint(total(recv(c)))\n",
            "4500000\n55\n210\n",
        ),
        (
            "a_blocked_fiber_keeps_its_stack",
            "let c = Channel(1)\nlet f = spawn { let mine = made(50)\n  let n = recv(c)\n  wrapAdd(total(mine), n) }\nprint(churn(500000))\nsend(c, 7)\nprint(await(f))\n",
            "4500000\n1282\n",
        ),
        (
            "a_sleeping_fiber_keeps_its_stack",
            "let f = spawn { let mine = made(50)\n  sleep(200)\n  total(mine) }\nprint(churn(1000000))\nprint(await(f))\n",
            "9000000\n1275\n",
        ),
        (
            "fibers_trading_lists_through_channels",
            "let a = Channel(1)\nlet b = Channel(1)\nlet f = spawn { let x = recv(a)\n  let c = churn(300000)\n  send(b, listAppend(x, c))\n  total(x) }\nsend(a, made(30))\nlet d = churn(300000)\nprint(d)\nprint(total(recv(b)))\nprint(await(f))\n",
            "2700000\n2700465\n465\n",
        ),
        (
            "a_thread_that_never_allocates_only_delays_collection",
            "let f = spawn churn(500000)\nprint(range(0, 20000000) |> fold(0, fn(sum, i) => wrapAdd(sum, i)))\nprint(await(f))\n",
            "199999990000000\n4500000\n",
        ),
        (
            "many_fibers_at_once",
            "let f1 = spawn wrapAdd(churn(200000), total(made(10)))\nlet f2 = spawn wrapAdd(churn(200000), total(made(20)))\nlet f3 = spawn wrapAdd(churn(200000), total(made(30)))\nlet f4 = spawn wrapAdd(churn(200000), total(made(40)))\nprint(await(f1))\nprint(await(f2))\nprint(await(f3))\nprint(await(f4))\n",
            "1800055\n1800210\n1800465\n1800820\n",
        ),
    ];
    let mut wrong = Vec::new();
    for (name, body, expected) in cases {
        for memory in ["gc", "arc"] {
            let result = run_within_budget(name, &format!("{LIVE_PRELUDE}{body}"), memory)?;
            if result.as_deref() != Ok(expected) {
                wrong.push(format!("{name}/{memory}: {result:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}
