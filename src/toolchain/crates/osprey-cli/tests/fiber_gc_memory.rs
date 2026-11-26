//! Garbage must be reclaimed however a program uses threads: a runaway here costs
//! the whole machine, so every run is killed at its memory budget. [MEM-BACKENDS]
#![cfg(unix)]

#[path = "common/bounded.rs"]
mod bounded;
#[path = "common/bounded_program.rs"]
mod bounded_program;

use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// About 2.5 GB of garbage per `churn()`, none of it retained.
const CHURN: &str = "fn churn() = range(0, 2000000) |> fold(0, fn(sum, i) => wrapAdd(sum, length(listAppend([1, 2, 3, 4, 5, 6, 7, 8], i))))\nprint(length(listAppend([1, 2], 3)))\n";
/// What a program printed, or why it was stopped or failed.
fn answer(name: &str, source: &str, memory: &str) -> io::Result<Result<String, String>> {
    let flag = format!("--memory={memory}");
    let run = bounded_program::run_within_budget(
        &format!("{name}_{memory}"),
        source,
        &["--run", "--quiet", &flag],
        &[],
    )?;
    Ok(match (&run.stopped, run.code) {
        (Some(reason), _) => Err(reason.clone()),
        (None, Some(0)) => Ok(run.stdout),
        (None, _) => Err(format!("{}: {}", run.summary(), run.stderr)),
    })
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
    check_cases(CHURN, cases)
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
            "fn buffer(c) = {\n  send(c, made(10))\n  send(c, made(20))\n  let f = spawn churn(500000)\n  print(\"${await(f)}\\n${total(recv(c))}\\n${total(recv(c))}\")\n}\nmatch Channel(4) {\n  Success { value } => buffer(value)\n  Error { message } => print(message)\n}\n",
            "4500000\n55\n210\n",
        ),
        (
            "a_blocked_fiber_keeps_its_stack",
            "fn block(c) = {\n  let f = spawn { let mine = made(50)\n    let n = recv(c)\n    wrapAdd(total(mine), n) }\n  print(churn(500000))\n  send(c, 7)\n  print(await(f))\n}\nmatch Channel(1) {\n  Success { value } => block(value)\n  Error { message } => print(message)\n}\n",
            "4500000\n1282\n",
        ),
        (
            "a_sleeping_fiber_keeps_its_stack",
            "let f = spawn { let mine = made(50)\n  sleep(200)\n  total(mine) }\nprint(churn(1000000))\nprint(await(f))\n",
            "9000000\n1275\n",
        ),
        (
            "fibers_trading_lists_through_channels",
            "fn trade(a, b) = {\n  let f = spawn { let x = recv(a)\n    let c = churn(300000)\n    send(b, listAppend(x, c))\n    total(x) }\n  send(a, made(30))\n  let d = churn(300000)\n  print(\"${d}\\n${total(recv(b))}\\n${await(f)}\")\n}\nmatch Channel(1) {\n  Success { value } => match Channel(1) {\n    Success { value: b } => trade(value, b)\n    Error { message } => print(message)\n  }\n  Error { message } => print(message)\n}\n",
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
    check_cases(LIVE_PRELUDE, cases)
}

fn check_cases<const N: usize>(prelude: &str, cases: [(&str, &str, &str); N]) -> io::Result<()> {
    let mut wrong = Vec::new();
    for (name, body, expected) in cases {
        for memory in ["gc", "arc"] {
            let result = answer(name, &format!("{prelude}{body}"), memory)?;
            if result.as_deref() != Ok(expected) {
                wrong.push(format!("{name}/{memory}: {result:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

const CEILING_MB: u64 = 64;
/// Exit status of a run stopped for memory, its heap ceiling or a thread's stack (`OSP_HEAP_EXHAUSTED_STATUS`).
const HEAP_EXHAUSTED: i32 = 71;
/// A list that only grows: every element stays reachable until the run ends.
const HOARD: &str = "fn hoard() = range(0, 1000000000000) |> fold([], fn(items, i) => listAppend(items, i))\nprint(\"started\")\n";

/// The `limit`, `peak` and `request` byte counts of the heap-limit record for `memory`.
fn heap_record(stderr: &str, memory: &str) -> Option<[u64; 3]> {
    let prefix = format!("osprey: heap limit exceeded: backend={memory} ");
    let fields = stderr.lines().find_map(|line| line.strip_prefix(&prefix))?;
    let mut values = fields
        .split(' ')
        .zip(["limit_bytes=", "peak_bytes=", "request_bytes="])
        .map(|(field, key)| field.strip_prefix(key)?.parse::<u64>().ok());
    Some([values.next()??, values.next()??, values.next()??])
}

/// Why a run under the ceiling was not what the case expects, if it was not.
fn ceiling_fault(memory: &str, run: &bounded::Bounded, survives: Option<&str>) -> Option<String> {
    let record = heap_record(&run.stderr, memory);
    let limit = CEILING_MB << 20;
    let right = match (survives, record) {
        (Some(printed), None) => run.code == Some(0) && run.stdout == printed,
        (None, Some([at, peak, request])) => {
            let within = peak <= at && peak + request + 64 > at;
            run.code == Some(HEAP_EXHAUSTED) && run.stdout == "started\n" && at == limit && within
        }
        _ => false,
    };
    (run.stopped.is_some() || !right).then(|| {
        format!(
            "stopped {:?}, exit {:?}, stdout {:?}, stderr {:?}",
            run.stopped, run.code, run.stdout, run.stderr
        )
    })
}

/// [MEM-HEAP-CEILING] A program that keeps everything it allocates ends at the
/// ceiling with the heap status, a record naming the limit and a peak within one
/// request of it, and the output it had printed. Without the ceiling such a run
/// grew until the OS killed other processes. Garbage never counts: 2.5 GB of
/// churn under the same ceiling finishes with its answer in both backends.
#[test]
fn a_heap_that_only_grows_ends_at_its_ceiling() -> io::Result<()> {
    let hoarding_fiber = format!("{HOARD}let f = spawn hoard()\nprint(length(await(f)))\n");
    let cases = [
        (
            "hoard_main",
            format!("{HOARD}print(length(hoard()))\n"),
            None,
        ),
        ("hoard_fiber", hoarding_fiber, None),
        (
            "churn_under_ceiling",
            format!("{CHURN}print(churn())\n"),
            Some("3\n18000000\n"),
        ),
    ];
    let ceiling = CEILING_MB.to_string();
    let mut wrong = Vec::new();
    for (name, source, survives) in &cases {
        for memory in ["gc", "arc"] {
            let run = bounded_program::run_within_budget(
                &format!("{name}_{memory}"),
                source,
                &["--run", "--quiet", &format!("--memory={memory}")],
                &[("OSPREY_HEAP_MAX_MB", &ceiling)],
            )?;
            if let Some(fault) = ceiling_fault(memory, &run, *survives) {
                wrong.push(format!("{name}/{memory}: {fault}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

/// Non-tail recursion far past any thread's stack: each frame waits on the next.
const DEEP: &str = "fn depth(n) = match n {\n    0 => 0\n    _ => satAdd(1, depth(satSub(n, 1)))\n}\nprint(\"started\")\n";

/// [FAILURE-EXPLICIT] Recursion that exhausts a thread's stack ends the run with
/// the memory status and a record naming the cause, on the main thread and in a
/// fiber, in every backend. It used to die by SIGSEGV with nothing printed (#363).
#[test]
fn recursion_past_the_stack_ends_by_name() -> io::Result<()> {
    let cases = [
        ("deep_main", format!("{DEEP}print(depth(100000000))\n")),
        (
            "deep_fiber",
            format!("{DEEP}let f = spawn depth(100000000)\nprint(await(f))\n"),
        ),
    ];
    let mut wrong = Vec::new();
    for (name, source) in &cases {
        for memory in ["gc", "arc"] {
            let run = bounded_program::run_within_budget(
                &format!("{name}_{memory}"),
                source,
                &["--run", "--quiet", &format!("--memory={memory}")],
                &[],
            )?;
            let named = run.stderr.contains(
                "osprey: stack exhausted: a call went deeper than the thread's stack allows",
            );
            if run.stopped.is_some()
                || run.code != Some(HEAP_EXHAUSTED)
                || run.stdout != "started\n"
                || !named
            {
                wrong.push(format!(
                    "{name}/{memory}: stopped {:?}, exit {:?}, stdout {:?}, stderr {:?}",
                    run.stopped, run.code, run.stdout, run.stderr
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

/// How long a case waits for the compiled program to prove it is running.
const START_BUDGET: Duration = Duration::from_secs(60);
/// How long a case waits for the program to end once its runner was killed.
const REAP_BUDGET: Duration = Duration::from_secs(5);
/// How often a case polls the marker file and the program pid.
const POLL: Duration = Duration::from_millis(50);

/// [RUN-PARENT-DEATH] Killing `osprey --run` ends the program it started, on
/// every kill route a test runner uses: an orphaned runaway is what took the
/// machine down. The program proves it runs by touching a marker file, then
/// sleeps far past every budget; whatever killed the osprey runner, the runner
/// may not die alone. Every case names the sleeper that outlived it.
#[test]
fn killing_the_runner_ends_the_program() -> io::Result<()> {
    let mut wrong = Vec::new();
    for memory in ["gc", "arc"] {
        for signal in ["KILL", "TERM", "INT", "HUP"] {
            let case = format!("{memory}_{signal}");
            if let Some(fault) = kill_case(&case, memory, signal)? {
                wrong.push(format!("{memory}/{signal}: {fault}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

/// One backend/signal case: why it failed, or `None` when the program ended
/// because its runner did.
fn kill_case(case: &str, memory: &str, signal: &str) -> io::Result<Option<String>> {
    let osp = std::env::temp_dir().join(format!("osprey_kill_{case}.osp"));
    let marker = std::env::temp_dir().join(format!("osprey_kill_{case}.marker"));
    write_case_files(&osp, &marker)?;
    let mut child = spawn_runner(&osp, memory)?;
    let runner = child.id();
    let (program, fault) = if poll(|| marker.exists(), START_BUDGET) {
        case_fault(signal, runner, &mut child)?
    } else {
        (None, Some("the program never started".to_string()))
    };
    cleanup(&mut child, runner, program, [&osp, &marker]);
    Ok(fault)
}

/// After the marker proved the program runs: find its pid, signal only the
/// runner, and require the program pid to be gone within `REAP_BUDGET`. Returns
/// the program pid for cleanup, plus why the case failed, if it did.
fn case_fault(
    signal: &str,
    runner: u32,
    child: &mut Child,
) -> io::Result<(Option<u32>, Option<String>)> {
    let Some(pid) = program_pid(runner) else {
        return Ok((
            None,
            Some("the program pid was not uniquely identifiable".to_string()),
        ));
    };
    signal_runner(signal, runner)?;
    let _ = child.wait();
    let gone = || match Command::new("kill").arg("-0").arg(pid.to_string()).output() {
        Ok(run) => !run.status.success(),
        Err(_) => false, // cannot probe: treat the program as alive and keep waiting
    };
    let fault = (!poll(gone, REAP_BUDGET)).then(|| "the program outlived its runner".to_string());
    Ok((Some(pid), fault))
}

/// Writes the sleeper program — touch `marker`, then sleep far past every
/// budget — after clearing any stale marker from an earlier identical case.
fn write_case_files(file: &Path, marker: &Path) -> io::Result<()> {
    let _ = std::fs::remove_file(marker);
    std::fs::write(
        file,
        format!(
            "match writeFile(\"{}\", \"up\") {{\n  Success {{ value }} => sleep(600000)\n  Error {{ message }} => print(message)\n}}\n",
            marker.display()
        ),
    )
}

/// Spawns `osprey <file> --run --quiet --memory=<memory>` with every stream
/// null, leading its own process group so cleanup can end the whole group.
fn spawn_runner(file: &Path, memory: &str) -> io::Result<Child> {
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(file)
        .arg("--run")
        .arg("--quiet")
        .arg(format!("--memory={memory}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
}

/// The one process whose parent is `runner` — the compiled program, when unique.
fn program_pid(runner: u32) -> Option<u32> {
    let listing = Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
        .ok()?;
    let pids: Vec<u32> = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter_map(|line| line.trim().split_once(char::is_whitespace))
        .filter(|(_, ppid)| ppid.trim().parse::<u32>() == Ok(runner))
        .filter_map(|(pid, _)| pid.trim().parse::<u32>().ok())
        .collect();
    match pids.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// Delivers `signal` to the osprey runner only; its exit status is beside the
/// point, so only a failure to run `kill` propagates.
fn signal_runner(signal: &str, runner: u32) -> io::Result<()> {
    let _ = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(runner.to_string())
        .output()?;
    Ok(())
}

/// Whether `just` turns true within `budget`, polled every `POLL`.
fn poll(just: impl Fn() -> bool, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if just() {
            return true;
        }
        std::thread::sleep(POLL);
    }
    false
}

/// Ends the runner's whole process group and the program pid when known, then
/// removes the case's files and reaps the runner — whatever happened above, so
/// no sleeper can outlive the test itself.
fn cleanup(child: &mut Child, runner: u32, program: Option<u32>, files: [&Path; 2]) {
    let _ = Command::new("kill")
        .args(["-9", &format!("-{runner}")])
        .output();
    if let Some(pid) = program {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
    }
    for file in files {
        let _ = std::fs::remove_file(file);
    }
    let _ = child.wait();
}

/// Runs a program that leaves a marker and prints `ran`, under a compiler told
/// that `runner` started it. Returns whether the marker appeared, the exit code
/// and what the compiler wrote to standard error.
fn named_run(case: &str, runner: u32) -> io::Result<(bool, Option<i32>, String)> {
    let osp = std::env::temp_dir().join(format!("osprey_named_{case}.osp"));
    let marker = std::env::temp_dir().join(format!("osprey_named_{case}.marker"));
    let _ = std::fs::remove_file(&marker);
    std::fs::write(
        &osp,
        format!(
            "match writeFile(\"{}\", \"up\") {{\n  Success {{ value }} => print(\"ran\")\n  Error {{ message }} => print(message)\n}}\n",
            marker.display()
        ),
    )?;
    let run = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&osp)
        .args(["--run", "--quiet"])
        .env("OSPREY_RUN_PARENT", runner.to_string())
        .stdin(Stdio::null())
        .output()?;
    let ran = marker.exists();
    let _ = std::fs::remove_file(&osp);
    let _ = std::fs::remove_file(&marker);
    Ok((
        ran,
        run.status.code(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
    ))
}

/// [RUN-PARENT-DEATH] A compiler whose named runner is already gone starts no
/// program: a runner killed while its compiler was still building left that
/// compiler to run the program with nothing watching it. A runner that is alive
/// is no reason to refuse, whether it is the compiler's parent or a process
/// further up that the name was inherited from.
#[test]
fn a_compiler_whose_runner_is_gone_starts_no_program() -> io::Result<()> {
    let mut finished = Command::new("true").spawn()?;
    let dead = finished.id();
    let _ = finished.wait()?;
    let (ran, code, stderr) = named_run("dead", dead)?;
    assert!(!ran, "the program ran under a dead runner: {stderr}");
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("OSPREY_RUN_PARENT") && stderr.contains(&dead.to_string()),
        "{stderr}"
    );
    let parent = named_run("parent", std::process::id())?;
    assert_eq!((parent.0, parent.1), (true, Some(0)), "{}", parent.2);
    let inherited = named_run("inherited", std::os::unix::process::parent_id())?;
    assert_eq!(
        (inherited.0, inherited.1),
        (true, Some(0)),
        "{}",
        inherited.2
    );
    Ok(())
}
