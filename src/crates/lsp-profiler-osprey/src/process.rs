//! Recognising Osprey processes [PROF-PROCESSES]. A process is an Osprey
//! program when its endpoint answers `hello` [PROF-ATTACH-ENDPOINT]; the reply
//! carries the runtime version the listing shows.

use std::io::ErrorKind;
use std::path::Path;

use lsp_profiler::{Classification, ProcessCandidate};

use crate::endpoint::{self, Endpoints};

/// Shown for an Osprey program whose endpoint exists but did not answer.
const NO_ANSWER: &str = "the program's diagnostics endpoint did not answer";

pub(crate) fn classify(
    endpoints: &Endpoints,
    candidates: &[ProcessCandidate],
) -> Vec<(usize, Classification)> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let socket = endpoints.socket_for(candidate.pid)?;
            classification(candidate, &socket).map(|found| (index, found))
        })
        .collect()
}

/// `None` when nothing serves this pid's socket: no file, or a stale file a
/// crashed program left for a pid the OS has since reused.
fn classification(candidate: &ProcessCandidate, socket: &Path) -> Option<Classification> {
    let base = Classification {
        executable: candidate.exe.as_ref().map(|exe| exe.display().to_string()),
        entry: Some(candidate.exe_basename()),
        requires_elevation: candidate.owned_by_other_user,
        ..Classification::default()
    };
    match endpoint::probe(socket) {
        Ok(hello) => Some(Classification {
            runtime_version: Some(hello.runtime_version),
            ..base
        }),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ) =>
        {
            None
        }
        Err(_) => Some(Classification {
            unattachable_reason: Some(NO_ANSWER.to_owned()),
            ..base
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    fn candidate(pid: u32) -> ProcessCandidate {
        ProcessCandidate {
            pid,
            ppid: None,
            name: "app".to_owned(),
            exe: Some(PathBuf::from("/bin/app")),
            cmd: Vec::new(),
            cwd: None,
            owned_by_other_user: false,
            child_of_current: false,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("osprey-classify-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_pid_without_a_socket_is_not_osprey() {
        let endpoints = Endpoints::new(Some(scratch("none")));
        assert_eq!(classify(&endpoints, &[candidate(7)]), Vec::new());
    }

    #[test]
    fn a_stale_socket_nobody_serves_is_not_osprey() {
        let dir = scratch("stale");
        drop(UnixListener::bind(dir.join("8.sock")).unwrap()); // the file stays, unserved
        assert_eq!(
            classify(&Endpoints::new(Some(dir)), &[candidate(8)]),
            Vec::new()
        );
    }

    #[test]
    fn a_socket_that_never_answers_is_osprey_but_unattachable() {
        let dir = scratch("silent");
        let _silent = UnixListener::bind(dir.join("9.sock")).unwrap(); // accepts, never replies
        let found = classify(&Endpoints::new(Some(dir)), &[candidate(5), candidate(9)]);
        let [(index, class)] = found.as_slice() else {
            panic!("one Osprey process: {found:?}")
        };
        assert_eq!(
            (*index, class.unattachable_reason.as_deref()),
            (1, Some(NO_ANSWER))
        );
        assert_eq!(
            (class.entry.as_deref(), class.executable.as_deref()),
            (Some("app"), Some("/bin/app"))
        );
    }
}
