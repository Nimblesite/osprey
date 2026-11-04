//! The CPU backend [PROF-CPU-BACKEND]: each attach opens one live-attach
//! session on the program's endpoint [PROF-ATTACH-PROTOCOL].

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use lsp_profiler::wire::CpuCapabilities;
use lsp_profiler::{CpuBackend, SamplerConfig, SamplerError, SamplerHandle};
use osprey_profiler::Image;

use crate::endpoint::{Connection, Endpoints};
use crate::protocol::{Hello, Message, PROTOCOL};
use crate::relay;
use crate::session::LiveSession;

/// How long connecting, `hello` and `cpu_start` may take together.
const ATTACH_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a stop waits for the last batch: the endpoint sends it at once,
/// and symbolizing it is one symbolizer run.
const FINISH_TIMEOUT: Duration = Duration::from_secs(10);
/// The runtime name a refused process is reported against.
const RUNTIME: &str = "Osprey";

#[derive(Debug)]
pub(crate) struct OspreyCpu {
    endpoints: Endpoints,
}

impl OspreyCpu {
    pub(crate) fn new(endpoints: Endpoints) -> Self {
        Self { endpoints }
    }

    /// The socket `pid` serves. Without one, a live pid is not an Osprey
    /// program and a dead one is not a process [PROF-ERRORS].
    fn endpoint_of(&self, pid: u32) -> Result<PathBuf, SamplerError> {
        self.endpoints
            .socket_for(pid)
            .filter(|socket| socket.exists())
            .ok_or_else(|| missing(pid))
    }
}

#[async_trait]
impl CpuBackend for OspreyCpu {
    fn capabilities(&self) -> CpuCapabilities {
        CpuCapabilities {
            live_progress: true,
            line_level: true,
            native_frames: true,
            cooperative: false,
        }
    }

    async fn attach(&self, config: &SamplerConfig) -> Result<SamplerHandle, SamplerError> {
        let socket = self.endpoint_of(config.pid)?;
        let (connection, hello, images) =
            tokio::time::timeout(ATTACH_TIMEOUT, start(&socket, config))
                .await
                .map_err(|elapsed| {
                    SamplerError::AttachFailed(format!("the program did not answer: {elapsed}"))
                })??;
        let parts = SamplerHandle::channel(config.pid, hello.runtime_version, FINISH_TIMEOUT);
        let session = LiveSession::new(Path::new(&hello.exe), images, config.include_native);
        drop(tokio::spawn(relay::run(
            connection,
            session,
            parts.sink,
            parts.done,
            config.duration,
        )));
        Ok(parts.handle)
    }
}

fn missing(pid: u32) -> SamplerError {
    if lsp_profiler::process::exists(pid) {
        SamplerError::UnsupportedRuntime {
            pid,
            runtime: RUNTIME.to_owned(),
        }
    } else {
        SamplerError::ProcessNotFound(pid)
    }
}

/// Connect, check the protocol, and start sampling at the requested rate.
async fn start(
    socket: &Path,
    config: &SamplerConfig,
) -> Result<(Connection, Hello, Vec<Image>), SamplerError> {
    let mut connection = Connection::open(socket)
        .await
        .map_err(|error| refused(config.pid, &error))?;
    let hello = greet(&mut connection).await?;
    connection
        .send(&format!("cpu_start {}", config.sample_rate))
        .await
        .map_err(|error| lost(&error))?;
    match connection.next().await.map_err(|error| lost(&error))? {
        Some(Message::CpuStarted(started)) => Ok((connection, hello, started.images)),
        Some(Message::Error(error)) => Err(SamplerError::AttachFailed(format!(
            "{}: {}",
            error.code, error.message
        ))),
        other => Err(SamplerError::AttachFailed(format!(
            "cpu_start got no cpu_started: {other:?}"
        ))),
    }
}

async fn greet(connection: &mut Connection) -> Result<Hello, SamplerError> {
    connection
        .send("hello")
        .await
        .map_err(|error| lost(&error))?;
    match connection.next().await.map_err(|error| lost(&error))? {
        Some(Message::Hello(hello)) if hello.protocol == PROTOCOL => Ok(hello),
        Some(Message::Hello(hello)) => Err(SamplerError::AttachFailed(format!(
            "the program speaks live-attach protocol {}, this server {PROTOCOL}",
            hello.protocol
        ))),
        other => Err(SamplerError::AttachFailed(format!(
            "hello got no answer: {other:?}"
        ))),
    }
}

/// A socket nobody accepts on is a stale file: a SIGKILL left it for a program
/// now gone, or for a pid now running something else. One we may not open
/// needs the program's own user.
fn refused(pid: u32, error: &io::Error) -> SamplerError {
    match error.kind() {
        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound => missing(pid),
        io::ErrorKind::PermissionDenied => SamplerError::PermissionDenied(format!(
            "the endpoint of PID {pid} belongs to another user: {error}"
        )),
        _ => lost(error),
    }
}

fn lost(error: &io::Error) -> SamplerError {
    SamplerError::AttachFailed(format!("the endpoint connection failed: {error}"))
}

#[cfg(test)]
#[path = "cpu_tests.rs"]
mod tests;
