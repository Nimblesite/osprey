//! Bound retained request data while a document's inference is running.

use super::{RequestId, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
struct Held {
    bytes: AtomicUsize,
    freed: Notify,
}

#[derive(Default)]
pub(super) struct Budget(Arc<Held>);

pub(super) struct Reservation {
    held: Arc<Held>,
    bytes: usize,
}

impl Budget {
    /// Room for a request now, or `None` while the queue is full.
    pub(super) fn try_reserve(
        &self,
        id: Option<&RequestId>,
        method: &str,
        params: &Value,
    ) -> Option<Reservation> {
        self.take(size(id, method, params))
    }

    /// Room for a notification, waiting until queued work frees it: a
    /// document edit is never dropped, and never ends the session.
    pub(super) async fn reserve(&self, method: &str, params: &Value) -> Reservation {
        let bytes = size(None, method, params);
        loop {
            let freed = self.0.freed.notified();
            if let Some(reservation) = self.take(bytes) {
                return reservation;
            }
            freed.await;
        }
    }

    /// A message larger than the whole budget is admitted alone.
    fn take(&self, bytes: usize) -> Option<Reservation> {
        let admitted = self
            .0
            .bytes
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |held| {
                held.checked_add(bytes)
                    .filter(|sum| *sum <= MAX_RETAINED_BYTES || held == 0)
            });
        admitted.is_ok().then(|| Reservation {
            held: Arc::clone(&self.0),
            bytes,
        })
    }
}

fn size(id: Option<&RequestId>, method: &str, params: &Value) -> usize {
    let id = match id {
        Some(RequestId::String(text)) => text.len(),
        Some(RequestId::Number(_)) => size_of::<i64>(),
        None => 0,
    };
    method.len() + params.to_string().len() + id
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let _ = self.held.bytes.fetch_sub(self.bytes, Ordering::SeqCst);
        self.held.freed.notify_waiters();
    }
}
