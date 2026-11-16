//! Queued requests a `$/cancelRequest` can still reach [LSP-LIFECYCLE].
//!
//! A request is registered for cancellation only when it starts, so without
//! this a cancelled request still ran its whole analysis when its turn came.
//! Holds at most the queue's capacity: an entry leaves when the request is
//! dequeued or refused.

use lspkit_server::{JsonRpcError, Message, RequestId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tokio_util::sync::CancellationToken;

/// LSP `RequestCancelled`: the client cancelled the request.
const REQUEST_CANCELLED: i32 = -32800;

#[derive(Clone, Default)]
pub(super) struct Pending(Arc<Mutex<HashMap<RequestId, CancellationToken>>>);

impl Pending {
    pub(super) fn insert(&self, id: &RequestId) -> CancellationToken {
        let token = CancellationToken::new();
        drop(self.lock().insert(id.clone(), token.clone()));
        token
    }

    pub(super) fn remove(&self, id: &RequestId) {
        drop(self.lock().remove(id));
    }

    pub(super) fn cancel(&self, id: &RequestId) {
        if let Some(token) = self.lock().get(id) {
            token.cancel();
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<RequestId, CancellationToken>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The reply to a request cancelled before it ran.
pub(super) fn cancelled(id: RequestId) -> Message {
    Message::response_error(
        id,
        JsonRpcError::new(REQUEST_CANCELLED, "request cancelled"),
    )
}
