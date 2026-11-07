//! Ordered analysis away from the lifecycle transport [LSP-LIFECYCLE].

use super::{cancel_id, notify, request, OspreyEngine, ProfilerHost, ServerError, SharedWriter};
use lspkit_server::{DiagnosticsBus, Dispatcher, Message, RequestId};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

mod budget;
mod sweep;

const MAX_QUEUED_MESSAGES: usize = 256;

struct Work {
    id: Option<RequestId>,
    method: String,
    params: Value,
    _reservation: budget::Reservation,
}

#[derive(Clone)]
struct Context {
    engine: OspreyEngine,
    bus: DiagnosticsBus,
    dispatcher: Dispatcher,
    writer: SharedWriter,
    stopped: CancellationToken,
    /// Wakes the repository sweep after a message may have queued files.
    wake: Arc<Notify>,
    sweep: Arc<tokio::sync::Mutex<sweep::Sweep>>,
}

pub(super) struct Queue {
    sender: Sender<Work>,
    budget: budget::Budget,
    context: Context,
    task: JoinHandle<()>,
    sweeper: JoinHandle<()>,
}

impl Queue {
    pub(super) fn start(
        engine: &OspreyEngine,
        bus: &DiagnosticsBus,
        dispatcher: &Dispatcher,
        writer: &SharedWriter,
    ) -> Self {
        let context = Context::new(engine, bus, dispatcher, writer);
        let (sender, receiver) = channel(MAX_QUEUED_MESSAGES);
        let runtime = tokio::runtime::Handle::current();
        let worker = context.clone();
        let sweeping = runtime.clone();
        let task = tokio::task::spawn_blocking(move || runtime.block_on(worker.run(receiver)));
        let sweeper = context.clone();
        let sweeper = tokio::task::spawn_blocking(move || sweeping.block_on(sweeper.sweep()));
        Self {
            sender,
            budget: budget::Budget::default(),
            context,
            task,
            sweeper,
        }
    }

    fn submit(
        &self,
        id: Option<RequestId>,
        method: String,
        params: Value,
    ) -> Result<(), ServerError> {
        let reservation = self.budget.reserve(id.as_ref(), &method, &params)?;
        self.sender
            .try_send(Work {
                id,
                method,
                params,
                _reservation: reservation,
            })
            .map_err(|error| match error {
                TrySendError::Full(_work) => ServerError::AnalysisBacklog,
                TrySendError::Closed(_work) => {
                    ServerError::AnalysisStopped("channel closed".into())
                }
            })
    }

    pub(super) async fn route(
        &self,
        message: Message,
        host: &ProfilerHost,
    ) -> Result<bool, ServerError> {
        let params = message.params.unwrap_or(Value::Null);
        match (message.id, message.method) {
            (Some(id), Some(method)) => self.route_request(id, method, params, host).await?,
            (None, Some(method)) => return self.route_notification(method, params, host),
            _ => {}
        }
        Ok(false)
    }

    async fn route_request(
        &self,
        id: RequestId,
        method: String,
        params: Value,
        host: &ProfilerHost,
    ) -> Result<(), ServerError> {
        if host.dispatch(&id, &method, &params) {
            return Ok(());
        }
        if method != "shutdown" {
            return self.submit(Some(id), method, params);
        }
        let reply = request(&self.context.dispatcher, &id, &method, params).await;
        let _ = self.context.writer.write_message(&reply).await;
        Ok(())
    }

    fn route_notification(
        &self,
        method: String,
        params: Value,
        host: &ProfilerHost,
    ) -> Result<bool, ServerError> {
        if method == "exit" {
            return Ok(true);
        }
        if method == "$/cancelRequest" {
            if let Some(id) = cancel_id(&params) {
                host.cancel(&id);
                self.context.dispatcher.cancel(&id);
            }
        } else {
            self.submit(None, method, params)?;
        }
        Ok(false)
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        self.context.stopped.cancel();
        self.context.wake.notify_one();
        self.context.engine.stop();
        self.task.abort();
        self.sweeper.abort();
    }
}

impl Context {
    fn new(
        engine: &OspreyEngine,
        bus: &DiagnosticsBus,
        dispatcher: &Dispatcher,
        writer: &SharedWriter,
    ) -> Self {
        Self {
            engine: engine.clone(),
            bus: bus.clone(),
            dispatcher: dispatcher.clone(),
            writer: Arc::clone(writer),
            stopped: CancellationToken::new(),
            wake: Arc::default(),
            sweep: Arc::default(),
        }
    }

    /// Serve messages in order. The repository sweep runs beside this loop,
    /// so requests never wait behind it [LSP-DIAGNOSTICS-SCOPE].
    async fn run(self, mut receiver: Receiver<Work>) {
        while let Some(work) = receiver.recv().await {
            if self.stopped.is_cancelled() {
                return;
            }
            self.handle(work).await;
            self.sweep.lock().await.show(&self).await;
            self.wake.notify_one();
        }
    }

    /// Check queued files one at a time beside the request loop, edited ones
    /// first, publishing each one still in scope.
    async fn sweep(self) {
        while !self.stopped.is_cancelled() {
            let next = super::scope::lock(&self.engine).next_swept();
            let Some((doc, siblings)) = next else {
                self.wake.notified().await;
                continue;
            };
            let uri = doc.as_str().to_string();
            let batch = super::diagnosed(&self.engine, doc.clone()).await;
            if siblings {
                // Analysing `doc` loads its projects, so siblings are known now.
                super::scope::queue_siblings(&self.engine, &doc);
            }
            if super::scope::lock(&self.engine).swept(&uri) {
                if let Some(batch) = batch {
                    self.bus.publish(batch).await;
                }
            }
            self.sweep.lock().await.show(&self).await;
        }
    }

    async fn handle(&self, work: Work) {
        if let Some(id) = work.id {
            if work.method == "initialize" {
                super::scope::lock(&self.engine).initialize(&work.params);
            }
            let reply = request(&self.dispatcher, &id, &work.method, work.params).await;
            if !self.stopped.is_cancelled() {
                let _ = self.writer.write_message(&reply).await;
            }
        } else {
            notify(
                &self.engine,
                &self.bus,
                &self.dispatcher,
                &work.method,
                work.params,
            )
            .await;
        }
    }
}
