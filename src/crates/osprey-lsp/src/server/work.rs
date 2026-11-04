//! Ordered analysis away from the lifecycle transport [LSP-LIFECYCLE].

use super::{cancel_id, notify, request, OspreyEngine, ProfilerHost, ServerError, SharedWriter};
use lspkit_server::{DiagnosticsBus, Dispatcher, Message, RequestId};
use serde_json::Value;
use tokio::sync::mpsc::{channel, error::TrySendError, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

mod budget;

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
}

pub(super) struct Queue {
    sender: Sender<Work>,
    budget: budget::Budget,
    context: Context,
    task: JoinHandle<()>,
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
        let task = tokio::task::spawn_blocking(move || runtime.block_on(worker.run(receiver)));
        Self {
            sender,
            budget: budget::Budget::default(),
            context,
            task,
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
        self.context.engine.stop();
        self.task.abort();
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
            writer: std::sync::Arc::clone(writer),
            stopped: CancellationToken::new(),
        }
    }

    async fn run(self, mut receiver: Receiver<Work>) {
        while let Some(work) = receiver.recv().await {
            if self.stopped.is_cancelled() {
                return;
            }
            self.handle(work).await;
        }
    }

    async fn handle(&self, work: Work) {
        if let Some(id) = work.id {
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
