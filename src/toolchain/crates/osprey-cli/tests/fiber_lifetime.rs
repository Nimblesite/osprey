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

/// The memory and time one compiled program may use, the compiler not counted.
const PROGRAM_ONLY: bounded::Budget = bounded::Budget {
    memory_mb: 1024,
    deadline: Duration::from_secs(120),
};

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
        let _ = host.arg(&program);
        host
    } else {
        Command::new(&program)
    };
    let run = bounded::run_command(command, stem, PROGRAM_ONLY);
    std::fs::remove_dir_all(&directory)?;
    run
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
