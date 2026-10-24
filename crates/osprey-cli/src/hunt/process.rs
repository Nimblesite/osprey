//! Bounded child processes: nothing a hunt spawns may run forever, neither
//! the engine past its budget nor a native replay of a program that never
//! exits [SYMEX-KLEE-INVOCATION] [SYMEX-REPLAY].

use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

/// Waits for `child` up to `limit`; past it the child is killed and `None` is
/// returned.
pub(crate) fn wait_bounded(
    child: &mut Child,
    limit: Duration,
) -> Result<Option<ExitStatus>, String> {
    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("child did not finish: {error}"))?
        {
            return Ok(Some(status));
        }
        if start.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[cfg(unix)]
    #[test]
    fn a_child_that_never_exits_is_killed_and_a_quick_one_is_awaited() {
        let mut slow = Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn");
        assert!(wait_bounded(&mut slow, Duration::from_millis(100))
            .expect("wait")
            .is_none());
        let mut quick = Command::new("/bin/sh")
            .args(["-c", "exit 3"])
            .spawn()
            .expect("spawn");
        let status = wait_bounded(&mut quick, Duration::from_secs(5))
            .expect("wait")
            .expect("exited");
        assert_eq!(status.code(), Some(3));
    }
}
