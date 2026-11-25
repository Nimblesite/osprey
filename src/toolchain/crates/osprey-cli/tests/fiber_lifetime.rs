//! When a fiber's memory is given back, in every memory backend and on wasm32,
//! measured on the compiled program alone. [CONCURRENCY-FIBER-LIFETIME]
//! [CONCURRENCY-WASM]
#![cfg(unix)]

#[path = "common/bounded.rs"]
mod bounded;

use std::io;
use std::process::Command;
use std::time::Duration;

/// Spawns and awaits fibers one after another, half answering ints and half
/// strings, so only one is ever live.
const FINISHED_FIBERS: &str = "fn ints(n) = range(0, n) |> fold(0, fn(sum, i) => wrapAdd(sum, await(spawn wrapAdd(i, 1))))\nfn texts(n) = range(0, n) |> fold(0, fn(sum, i) => wrapAdd(sum, length(await(spawn toString(i)))))\nprint(\"${ints(600000)} ${texts(600000)}\")\n";
const FINISHED_FIBERS_ANSWER: &str = "180000300000 3488890\n";
/// The same count of fibers for `--memory=ownership`, which refuses a string
/// handed out of a fiber ([MEM-OWNERSHIP-STATIC]), so every one answers an int.
const FINISHED_INT_FIBERS: &str = "fn ints(n) = range(0, n) |> fold(0, fn(sum, i) => wrapAdd(sum, await(spawn wrapAdd(i, 1))))\nprint(\"${ints(1200000)}\")\n";
const FINISHED_INT_FIBERS_ANSWER: &str = "720000600000\n";
/// What a program that keeps one finished fiber at a time may reach, a WASI
/// host included: the runtime, a collector's slack and the threads still
/// exiting, far below the 300 MiB a run that keeps all 1.2 million reached.
const FINISHED_FIBERS_PEAK_MB: u64 = 64;

/// [CONCURRENCY-FIBER-LIFETIME] [CONCURRENCY-WASM] A fiber nothing can await
/// again is reclaimed with its record, result and synchronisation, in every
/// backend and on wasm32. Every finished fiber stayed in the runtime's table for
/// the life of the process, outside the managed heap and its ceiling, so 1.2
/// million sequential `await(spawn ...)` reached 330 MiB (#359).
#[test]
fn finished_fibers_are_reclaimed_in_every_backend() -> io::Result<()> {
    let cases = [
        ("gc", "native", FINISHED_FIBERS, FINISHED_FIBERS_ANSWER),
        ("arc", "native", FINISHED_FIBERS, FINISHED_FIBERS_ANSWER),
        (
            "ownership",
            "native",
            FINISHED_INT_FIBERS,
            FINISHED_INT_FIBERS_ANSWER,
        ),
        ("arc", "wasm32", FINISHED_FIBERS, FINISHED_FIBERS_ANSWER),
        (
            "ownership",
            "wasm32",
            FINISHED_INT_FIBERS,
            FINISHED_INT_FIBERS_ANSWER,
        ),
    ];
    let mut wrong = Vec::new();
    for (memory, target, source, answer) in cases {
        let run = compiled_run(
            &format!("finished_fibers_{memory}_{target}"),
            source,
            memory,
            target,
        )?;
        let answered = run.code == Some(0) && run.stdout == answer;
        if !answered || run.peak_mb > FINISHED_FIBERS_PEAK_MB {
            wrong.push(format!(
                "{memory}/{target}: {}, stdout {:?}, stderr {:?}",
                run.summary(),
                run.stdout,
                run.stderr
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

/// Makes, fills and drains a fresh 256-slot channel one after another, half
/// carrying ints and half strings, so only one is ever live.
const FINISHED_CHANNELS: &str = "fn ints(n) = range(0, n) |> fold(0, fn(sum, i) => {\n  let box = Channel(256)\n  send(box, i)\n  wrapAdd(sum, recv(box))\n})\nfn texts(n) = range(0, n) |> fold(0, fn(sum, i) => {\n  let box = Channel(256)\n  send(box, toString(i))\n  wrapAdd(sum, length(recv(box)))\n})\nprint(\"${ints(300000)} ${texts(300000)}\")\n";
const FINISHED_CHANNELS_ANSWER: &str = "44999850000 1688890\n";
/// The same count of channels for `--memory=ownership`, which refuses a string
/// handed to a channel ([MEM-OWNERSHIP-STATIC]), so every one carries an int.
const FINISHED_INT_CHANNELS: &str = "fn ints(n) = range(0, n) |> fold(0, fn(sum, i) => {\n  let box = Channel(256)\n  send(box, i)\n  wrapAdd(sum, recv(box))\n})\nprint(\"${ints(600000)}\")\n";
const FINISHED_INT_CHANNELS_ANSWER: &str = "179999700000\n";

/// [CONCURRENCY-FIBER-LIFETIME] [CONCURRENCY-CHANNEL] [CONCURRENCY-WASM] A
/// channel nothing can name again is reclaimed with its buffer and
/// synchronisation, in every backend and on wasm32. Every channel stayed in the
/// runtime's table for the life of the process, so 600 thousand sequential
/// `Channel(256)` reached 1.4 GiB under ARC and 1.8 GiB under the GC.
#[test]
fn finished_channels_are_reclaimed_in_every_backend() -> io::Result<()> {
    let cases = [
        ("gc", "native", FINISHED_CHANNELS, FINISHED_CHANNELS_ANSWER),
        ("arc", "native", FINISHED_CHANNELS, FINISHED_CHANNELS_ANSWER),
        (
            "ownership",
            "native",
            FINISHED_INT_CHANNELS,
            FINISHED_INT_CHANNELS_ANSWER,
        ),
        ("arc", "wasm32", FINISHED_CHANNELS, FINISHED_CHANNELS_ANSWER),
        (
            "ownership",
            "wasm32",
            FINISHED_INT_CHANNELS,
            FINISHED_INT_CHANNELS_ANSWER,
        ),
    ];
    let mut wrong = Vec::new();
    for (memory, target, source, answer) in cases {
        let run = compiled_run(
            &format!("finished_channels_{memory}_{target}"),
            source,
            memory,
            target,
        )?;
        let answered = run.code == Some(0) && run.stdout == answer;
        if !answered || run.peak_mb > FINISHED_FIBERS_PEAK_MB {
            wrong.push(format!(
                "{memory}/{target}: {}, stdout {:?}, stderr {:?}",
                run.summary(),
                run.stdout,
                run.stderr
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

/// Fibers that share channels: one made per call and shared by two fibers (with
/// room for every value, as wasm32 runs a fiber to completion), one
/// at file scope shared with a fiber that outlives the statement, and 2000
/// more made, shared and dropped in a loop.
const SHARED_CHANNELS: &str = "fn produce(box, n) = range(0, n) |> fold(0, fn(sum, i) => {\n  send(box, i)\n  wrapAdd(sum, 1)\n})\nfn consume(box, n) = range(0, n) |> fold(0, fn(sum, _) => wrapAdd(sum, recv(box)))\nfn relay(n) = {\n  let box = Channel(64)\n  let maker = spawn produce(box, n)\n  let taker = spawn consume(box, n)\n  wrapAdd(await(maker), await(taker))\n}\nlet shared = Channel(2)\nlet late = spawn recv(shared)\nsend(shared, 7)\nprint(\"${relay(50)} ${relay(3)} ${await(late)} ${range(0, 2000) |> fold(0, fn(sum, _) => wrapAdd(sum, relay(2)))}\")\n";
const SHARED_CHANNELS_ANSWER: &str = "1275 6 7 6000\n";

/// [CONCURRENCY-CHANNEL-LIFETIME] [MEM-OWNERSHIP-STATIC-HANDLES] A channel is
/// shared by every fiber that names it and ends with its last sharer, in every
/// backend and on wasm32, leaving nothing live at exit. Ownership refused the
/// program once a channel became a managed value: it exists to be shared, so
/// its handle is the one value that mode lets a program share.
#[test]
fn shared_channels_end_with_their_last_sharer() -> io::Result<()> {
    let cases = [
        ("gc", "native", ""),
        ("arc", "native", "[osp-arc] exit: 0 live objects"),
        (
            "ownership",
            "native",
            "[osp-ownership] exit: 0 live objects",
        ),
        ("arc", "wasm32", "[osp-arc] exit: 0 live objects"),
        (
            "ownership",
            "wasm32",
            "[osp-ownership] exit: 0 live objects",
        ),
    ];
    let mut wrong = Vec::new();
    for (memory, target, nothing_live) in cases {
        let run = compiled_run(
            &format!("shared_channels_{memory}_{target}"),
            SHARED_CHANNELS,
            memory,
            target,
        )?;
        let answered = run.code == Some(0) && run.stdout == SHARED_CHANNELS_ANSWER;
        if !answered
            || !run.stderr.contains(nothing_live)
            || run.stderr.contains("shared:") && !run.stderr.contains("shared: 0 blocks")
        {
            wrong.push(format!(
                "{memory}/{target}: {}, stdout {:?}, stderr {:?}",
                run.summary(),
                run.stdout,
                run.stderr
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

/// The memory and time one compiled program may use, the compiler not counted.
const PROGRAM_ONLY: bounded::Budget = bounded::Budget {
    memory_mb: 1024,
    deadline: Duration::from_secs(120),
};

/// The switches that make ARC and ownership report what is live at exit.
const LEAK_REPORTS: [&str; 2] = ["OSPREY_ARC_DEBUG", "OSPREY_OWNERSHIP_DEBUG"];

/// Compiles `source` for `memory` and `target`, then runs only the program
/// under [`PROGRAM_ONLY`], so its peak is the program's own (a WASI host's for
/// wasm32), never the compiler's, which `--run` keeps resident meanwhile.
fn compiled_run(
    stem: &str,
    source: &str,
    memory: &str,
    target: &str,
) -> io::Result<bounded::Bounded> {
    let directory =
        std::env::temp_dir().join(format!("osprey_compiled_{}_{stem}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let file = directory.join(format!("{stem}.osp"));
    std::fs::write(&file, source)?;
    let wasm = target == "wasm32";
    let program = directory.join(if wasm { "program.wasm" } else { "program" });
    let built = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&file)
        .args([
            "--compile",
            "--quiet",
            &format!("--memory={memory}"),
            &format!("--target={target}"),
            "-o",
        ])
        .arg(&program)
        .output()?;
    assert!(
        built.status.success(),
        "{stem}: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let command = if wasm {
        let mut host =
            Command::new(std::env::var("OSPREY_WASM_RUN").unwrap_or_else(|_| "wasmtime".into()));
        let _ = host
            .args(LEAK_REPORTS.map(|name| format!("--env={name}=1")))
            .arg(&program);
        host
    } else {
        let mut native = Command::new(&program);
        let _ = native.envs(LEAK_REPORTS.map(|name| (name, "1")));
        native
    };
    let run = bounded::run_command(command, stem, PROGRAM_ONLY);
    std::fs::remove_dir_all(&directory)?;
    run
}

/// Twenty fibers each left waiting on a channel only it can name, then enough
/// time for every one to block before `main` ends.
const STRANDED_FIBERS: &str = "fn strand() = {\n  let box = Channel(1)\n  let waiting = spawn wrapAdd(recv(box), 1)\n  1\n}\nlet stranded = range(0, 20) |> fold(0, fn(sum, _) => wrapAdd(sum, strand()))\nsleep(300)\nprint(\"${stranded}\")\n";
const STRANDED_REPORT: &str = "fiber(s) blocked for ever: only blocked fibers can reach the channel each waits on [CONCURRENCY-DEADLOCK]";

/// [CONCURRENCY-DEADLOCK] A fiber waiting on a channel that only waiting fibers
/// can reach can never be woken. The tracing collector finds it by
/// reachability and the run names it on stderr; the program's own result is
/// untouched. Reference counting cannot tell the fiber's own reference from
/// one that could wake it, so ARC reports nothing, and says nothing false.
#[test]
fn fibers_blocked_for_ever_are_named_by_the_collector() -> io::Result<()> {
    let mut wrong = Vec::new();
    for (memory, reported) in [("gc", true), ("arc", false)] {
        let run = compiled_run(
            &format!("stranded_{memory}"),
            STRANDED_FIBERS,
            memory,
            "native",
        )?;
        let answered = run.code == Some(0) && run.stdout == "20\n";
        if !answered || run.stderr.contains(STRANDED_REPORT) != reported {
            wrong.push(format!(
                "{memory}: {}, stdout {:?}, stderr {:?}",
                run.summary(),
                run.stdout,
                run.stderr
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}

/// A program whose one fiber waits for a value no fiber will ever send.
const BLOCKED_FOREVER: &str = "let box = Channel(1)\nlet waiting = spawn recv(box)\nprint(\"started\")\nprint(await(waiting))\n";

/// [CONCURRENCY-WASM] On wasm32's one thread a wait no queued fiber can satisfy
/// could only be woken by another thread, and there is none, so the run stops by
/// name with status 70 instead of hanging, after what it had printed.
#[test]
fn a_wasm_wait_nothing_can_wake_stops_by_name() -> io::Result<()> {
    let mut wrong = Vec::new();
    for memory in ["arc", "ownership"] {
        let run = compiled_run(
            &format!("blocked_forever_{memory}"),
            BLOCKED_FOREVER,
            memory,
            "wasm32",
        )?;
        let named = run
            .stderr
            .contains("osprey: every fiber is blocked: this wait can never be woken");
        if run.stopped.is_some() || run.code != Some(70) || run.stdout != "started\n" || !named {
            wrong.push(format!(
                "{memory}: {}, stdout {:?}, stderr {:?}",
                run.summary(),
                run.stdout,
                run.stderr
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    Ok(())
}
