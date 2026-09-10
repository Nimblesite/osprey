//! Bounded example execution with concurrent pipe draining to avoid output deadlocks.
//! Implements [DOC-DOCTEST-HARNESS].

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

pub(super) fn capture(command: &mut Command) -> Result<Output, String> {
    let deadline = timeout()?;
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|error| error.to_string())?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = wait(&mut child, deadline);
    let stdout = stdout.join().map_err(|_panic| "example stdout reader failed")??;
    let stderr = stderr.join().map_err(|_panic| "example stderr reader failed")??;
    Ok(Output { status: status?, stdout, stderr })
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Result<Vec<u8>, String>> {
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut pipe = pipe.ok_or("example output pipe was not created")?;
        let _ = pipe.read_to_end(&mut output).map_err(|error| error.to_string())?;
        Ok(output)
    })
}

fn timeout() -> Result<Duration, String> {
    match std::env::var("OSPREY_DOCTEST_TIMEOUT_MS") {
        Ok(value) => value.parse::<u64>().ok().filter(|value| *value > 0).map(Duration::from_millis)
            .ok_or_else(|| "OSPREY_DOCTEST_TIMEOUT_MS must be a positive integer".into()),
        Err(std::env::VarError::NotPresent) => Ok(Duration::from_secs(30)),
        Err(error) => Err(error.to_string()),
    }
}

fn wait(child: &mut Child, timeout: Duration) -> Result<ExitStatus, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(5)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(result.err().map_or_else(|| format!("execution timed out after {} ms", timeout.as_millis()), |error| error.to_string()));
            }
        }
    }
}
