//! Mounts lspkit's shared profiler in the Osprey language server
//! [PROF-LSP-HOST] (docs/specs/tooling/0028-Profiler.md).
//!
//! The service owns every profiling decision. This module only routes the
//! `workspace/executeCommand` requests named `osprey.lspProfiler.<area>.<action>`
//! to it, answers each from a task of its own so a session that takes seconds
//! to stop never holds up editing, forwards the service's notifications, and
//! honours `$/cancelRequest` while an operation is in flight.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use lsp_profiler::runtime::Runtime;
use lsp_profiler::{commands, ProfilerConfig, ProfilerError, ProfilerEvents, ProfilerService};
use lsp_profiler_osprey::{OspreyConfig, OspreyRuntime};
use lspkit_server::{JsonRpcError, Message, RequestId};
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::server::SharedWriter;

/// The host prefix of every profiler command: `osprey.lspProfiler.<area>.<action>`.
pub(crate) const COMMAND_PREFIX: &str = "osprey";

/// JSON-RPC code of a cancelled request.
const REQUEST_CANCELLED: i32 = -32800;

/// Forwards the service's notifications (CPU progress) to the editor.
struct Events {
    writer: SharedWriter,
}

impl fmt::Debug for Events {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Events")
    }
}

#[async_trait]
impl ProfilerEvents for Events {
    async fn notify(&self, method: &'static str, params: Value) {
        drop(
            self.writer
                .write_message(&Message::notification(method, params))
                .await,
        );
    }
}

type Pending = Arc<Mutex<HashMap<RequestId, oneshot::Sender<()>>>>;

/// The mounted profiler: one service for the server's lifetime.
pub(crate) struct ProfilerHost {
    service: Arc<ProfilerService>,
    pending: Pending,
    writer: SharedWriter,
}

impl ProfilerHost {
    /// The service with the Osprey runtime, writing exports under the system
    /// temp directory.
    pub(crate) fn new(writer: &SharedWriter) -> Self {
        let config = ProfilerConfig {
            output_dir: std::env::temp_dir().join("osprey-profiler"),
            ..ProfilerConfig::default()
        };
        let runtime = Arc::new(OspreyRuntime::new(OspreyConfig::default()));
        let events = Arc::new(Events {
            writer: Arc::clone(writer),
        });
        Self {
            service: Arc::new(ProfilerService::new(vec![runtime], config, events)),
            pending: Arc::default(),
            writer: Arc::clone(writer),
        }
    }

    /// Route a profiler command; false when the request is not one. A routed
    /// request is answered later, from its own task.
    pub(crate) fn dispatch(&self, id: &RequestId, method: &str, params: &Value) -> bool {
        let Some((profiler_method, arguments)) = (method == "workspace/executeCommand")
            .then(|| profiler_call(params))
            .flatten()
        else {
            return false;
        };
        let (cancel, cancelled) = oneshot::channel();
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        pending.retain(|_, sender| !sender.is_closed());
        drop(pending.insert(id.clone(), cancel));
        let call = Call {
            service: Arc::clone(&self.service),
            writer: Arc::clone(&self.writer),
            id: id.clone(),
        };
        drop(tokio::spawn(call.complete(
            profiler_method,
            arguments,
            malformed_arguments(params),
            cancelled,
        )));
        true
    }

    /// Consume a `$/cancelRequest`; unknown and completed ids are harmless.
    pub(crate) fn cancel(&self, id: &RequestId) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(cancel) = pending.remove(id) {
            let _ = cancel.send(());
        }
    }

    /// Stop every session before the server exits.
    pub(crate) async fn shutdown(&self) {
        self.service.shutdown().await;
    }
}

/// One in-flight profiler request.
struct Call {
    service: Arc<ProfilerService>,
    writer: SharedWriter,
    id: RequestId,
}

impl Call {
    /// Answer exactly once: refused, cancelled, or the service's result.
    async fn complete(
        self,
        method: &'static str,
        params: Value,
        malformed: Option<&'static str>,
        mut cancelled: oneshot::Receiver<()>,
    ) {
        let reply = if let Some(reason) = malformed {
            answer(
                self.id,
                Err(ProfilerError::InvalidParams(reason.to_owned())),
            )
        } else {
            // Only a real cancel ends the wait: a dropped sender leaves the
            // request to finish.
            tokio::select! {
                biased;
                Ok(()) = &mut cancelled => Message::response_error(
                    self.id,
                    JsonRpcError::new(REQUEST_CANCELLED, "profiler operation cancelled"),
                ),
                result = self.service.handle(method, params) => answer(self.id, result),
            }
        };
        drop(self.writer.write_message(&reply).await);
    }
}

/// The response to a finished operation, with the protocol's error codes.
fn answer(id: RequestId, result: Result<Value, ProfilerError>) -> Message {
    match result {
        Ok(value) => Message::response(id, value),
        Err(error) => Message::response_error(
            id,
            JsonRpcError::new(
                i32::try_from(error.code()).unwrap_or(-32000),
                error.to_string(),
            ),
        ),
    }
}

/// The `initialize` result with the profiler's capabilities: the
/// `experimental.lspProfiler` block naming every mounted runtime, and the
/// commands every profiler operation travels as [PROF-LSP-HOST].
pub(crate) fn initialize_result(mut result: Value) -> Value {
    let runtime = OspreyRuntime::new(OspreyConfig::default());
    if let Some(capabilities) = result
        .get_mut("capabilities")
        .and_then(Value::as_object_mut)
    {
        drop(capabilities.insert(
            "executeCommandProvider".to_owned(),
            json!({ "commands": commands::command_names(COMMAND_PREFIX) }),
        ));
        drop(capabilities.insert(
            "experimental".to_owned(),
            json!({ "lspProfiler": { "runtimes": [runtime.info()] } }),
        ));
    }
    result
}

/// The protocol method and its params when `params` names a profiler command.
fn profiler_call(params: &Value) -> Option<(&'static str, Value)> {
    let command = params.get("command")?.as_str()?;
    let method = commands::method_for(COMMAND_PREFIX, command)?;
    let arguments = params
        .get("arguments")
        .and_then(Value::as_array)
        .and_then(|args| args.first().cloned())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    Some((method, arguments))
}

/// Why a profiler command's arguments are not exactly one params object
/// ([PROF-WIRE-METHODS]); `None` when they are, or when the list is absent.
fn malformed_arguments(params: &Value) -> Option<&'static str> {
    match params.get("arguments") {
        None | Some(Value::Null) => None,
        Some(Value::Array(list)) => match list.as_slice() {
            [Value::Object(_)] => None,
            [_] => Some("the profiler params argument must be a JSON object"),
            _ => Some("a profiler command takes exactly one argument, its params object"),
        },
        Some(_) => Some("a profiler command's arguments must be a list of one params object"),
    }
}

#[cfg(test)]
#[path = "profiler_tests.rs"]
mod tests;
