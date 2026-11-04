//! The session relay [PROF-SAMPLER]: endpoint batches become per-tick sample
//! batches until the session asks to stop, its duration runs out, or the
//! program exits [PROF-ATTACH-PROTOCOL].
//!
//! A reader task owns the read half and forwards whole messages, because an
//! NDJSON read is not safe to cancel mid-line; the relay can then wait on the
//! inbox and the stop clock together and still never lose a byte.

use std::time::Duration;

use lsp_profiler::ndjson;
use lsp_profiler::{SampleBatch, SamplerSink};
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tracing::{debug, warn};

use crate::endpoint::{self, Connection, Reader};
use crate::protocol::{CpuBatch, Message};
use crate::session::LiveSession;

/// How often the relay checks whether the session wants to stop.
const STOP_POLL: Duration = Duration::from_millis(50);
/// Messages read ahead of the relay.
const INBOX_DEPTH: usize = 64;

/// Run one session to its end, then report the sampler finished.
pub(crate) async fn run(
    connection: Connection,
    session: LiveSession,
    sink: SamplerSink,
    done: oneshot::Sender<()>,
    duration: Option<Duration>,
) {
    let (reader, writer) = connection.split();
    let (forward, inbox) = mpsc::channel(INBOX_DEPTH);
    let reading = tokio::spawn(read_messages(reader, forward));
    relay(
        writer,
        inbox,
        session,
        &sink,
        duration.map(|limit| Instant::now() + limit),
    )
    .await;
    reading.abort();
    let _ = done.send(());
}

async fn read_messages(mut reader: Reader, forward: mpsc::Sender<Message>) {
    while let Ok(Some(message)) = ndjson::read_message(&mut reader).await {
        if forward.send(message).await.is_err() {
            return;
        }
    }
}

/// The write half stays open until the end: closing it would read as a
/// disconnect, and the endpoint would end the session without its last batch.
async fn relay(
    mut writer: OwnedWriteHalf,
    mut inbox: mpsc::Receiver<Message>,
    mut session: LiveSession,
    sink: &SamplerSink,
    deadline: Option<Instant>,
) {
    let mut poll = tokio::time::interval(STOP_POLL);
    let mut stopping = false;
    loop {
        tokio::select! {
            message = inbox.recv() => match handle(message, session, sink).await {
                Some(next) => session = next,
                None => return,
            },
            _ = poll.tick(), if !stopping => {
                stopping = sink.stop_requested() || deadline.is_some_and(|at| Instant::now() >= at);
                if stopping && endpoint::send_line(&mut writer, "cpu_stop").await.is_err() {
                    return;
                }
            }
        }
    }
}

/// The session after `message`, or `None` once the session is over.
async fn handle(
    message: Option<Message>,
    session: LiveSession,
    sink: &SamplerSink,
) -> Option<LiveSession> {
    match message? {
        Message::CpuBatch(batch) => deliver(session, batch, sink).await,
        Message::CpuStopped(stopped) => {
            debug!(reason = %stopped.reason, "live-attach session ended");
            None
        }
        Message::Error(error) => {
            warn!(code = %error.code, "the endpoint refused a request");
            Some(session)
        }
        Message::Hello(_) | Message::CpuStarted(_) => Some(session),
    }
}

/// Symbolize on a blocking thread — the symbolizer is a subprocess — then hand
/// each tick to the session.
async fn deliver(session: LiveSession, batch: CpuBatch, sink: &SamplerSink) -> Option<LiveSession> {
    let (session, ticks) = tokio::task::spawn_blocking(move || {
        let mut owned = session;
        let ticks: Vec<SampleBatch> = owned.ingest(batch);
        (owned, ticks)
    })
    .await
    .ok()?;
    for tick in ticks {
        sink.sender.send(tick).await.ok()?;
    }
    Some(session)
}
