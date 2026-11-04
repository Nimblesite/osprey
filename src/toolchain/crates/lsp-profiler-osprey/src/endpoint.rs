//! Where an Osprey program's endpoint lives and how to reach it
//! [PROF-ATTACH-ENDPOINT]: `$TMPDIR/osprey-diagnostics-<uid>/<pid>.sock`.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream as BlockingStream;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use lsp_profiler::ndjson;
use sysinfo::{get_current_pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::io::{AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;

use crate::protocol::{Hello, Message};

/// How long a classification probe waits for `hello`.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);
/// The longest `hello` a probe reads; a real one is a few hundred bytes.
const PROBE_LINE_MAX: u64 = 64 * 1024;

/// The endpoint directory: configured, or the one every program opens.
#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    dir: Option<PathBuf>,
}

impl Endpoints {
    pub(crate) fn new(configured: Option<PathBuf>) -> Self {
        Self {
            dir: configured.or_else(default_dir),
        }
    }

    /// The socket `pid` serves, when the directory is known.
    pub(crate) fn socket_for(&self, pid: u32) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join(format!("{pid}.sock")))
    }
}

/// The directory as the runtime computes it: `/tmp` when `TMPDIR` is unset or
/// empty.
fn default_dir() -> Option<PathBuf> {
    let tmp = std::env::var_os("TMPDIR")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    current_uid().map(|uid| tmp.join(format!("osprey-diagnostics-{uid}")))
}

/// This process's uid, which names the directory. Read once.
fn current_uid() -> Option<u32> {
    static UID: OnceLock<Option<u32>> = OnceLock::new();
    *UID.get_or_init(|| {
        let pid = get_current_pid().ok()?;
        let mut system = System::new();
        let refresh = ProcessRefreshKind::nothing().with_user(UpdateKind::Always);
        let _ = system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh);
        system.process(pid)?.user_id().map(|uid| **uid)
    })
}

/// Ask the endpoint at `socket` who it is. Blocking: classification already
/// runs on a blocking thread.
pub(crate) fn probe(socket: &Path) -> io::Result<Hello> {
    let stream = BlockingStream::connect(socket)?;
    stream.set_read_timeout(Some(PROBE_TIMEOUT))?;
    stream.set_write_timeout(Some(PROBE_TIMEOUT))?;
    (&stream).write_all(b"hello\n")?;
    let mut line = String::new();
    let _ = BufReader::new((&stream).take(PROBE_LINE_MAX)).read_line(&mut line)?;
    match serde_json::from_str(&line) {
        Ok(Message::Hello(hello)) => Ok(hello),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no hello on the endpoint",
        )),
    }
}

/// The read half of a session's connection, framed as NDJSON.
pub(crate) type Reader = AsyncBufReader<OwnedReadHalf>;

/// One session's connection: request lines out, messages in.
#[derive(Debug)]
pub(crate) struct Connection {
    reader: Reader,
    writer: OwnedWriteHalf,
}

impl Connection {
    pub(crate) async fn open(socket: &Path) -> io::Result<Self> {
        let (read, writer) = UnixStream::connect(socket).await?.into_split();
        Ok(Self {
            reader: AsyncBufReader::new(read),
            writer,
        })
    }

    pub(crate) async fn send(&mut self, line: &str) -> io::Result<()> {
        send_line(&mut self.writer, line).await
    }

    /// The next message; `None` once the endpoint closed the connection.
    pub(crate) async fn next(&mut self) -> io::Result<Option<Message>> {
        ndjson::read_message(&mut self.reader).await
    }

    pub(crate) fn split(self) -> (Reader, OwnedWriteHalf) {
        (self.reader, self.writer)
    }
}

/// One request line [PROF-ATTACH-PROTOCOL].
pub(crate) async fn send_line(writer: &mut OwnedWriteHalf, line: &str) -> io::Result<()> {
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sockets_are_named_by_pid_inside_the_directory() {
        let endpoints = Endpoints::new(Some(PathBuf::from("/run/diag")));
        assert_eq!(
            endpoints.socket_for(42),
            Some(PathBuf::from("/run/diag/42.sock"))
        );
    }

    #[test]
    fn the_default_directory_is_the_users_own() {
        let dir = default_dir().expect("the current uid is readable");
        let name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        assert_eq!(
            name,
            format!("osprey-diagnostics-{}", current_uid().unwrap_or(u32::MAX))
        );
    }

    #[test]
    fn a_reply_that_is_not_hello_is_no_osprey_endpoint() {
        use std::os::unix::net::UnixListener;
        let socket = std::env::temp_dir().join(format!("osprey-probe-{}.sock", std::process::id()));
        drop(std::fs::remove_file(&socket));
        let listener = UnixListener::bind(&socket).unwrap();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            drop(BufReader::new(stream.try_clone().unwrap()).read_line(&mut request));
            stream
                .write_all(b"{\"kind\":\"cpu_stopped\",\"reason\":\"exit\"}\n")
                .unwrap();
        });
        let error = probe(&socket).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        peer.join().unwrap();
        drop(std::fs::remove_file(&socket));
    }

    #[test]
    fn a_probe_of_nothing_fails_without_hanging() {
        let missing = std::env::temp_dir().join("osprey-diagnostics-test-missing.sock");
        assert!(probe(&missing).is_err());
    }
}
