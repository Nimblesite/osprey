//! Bound retained request data while a document's inference is running.

use super::{RequestId, ServerError, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Budget(Arc<AtomicUsize>);

pub(super) struct Reservation {
    retained: Arc<AtomicUsize>,
    bytes: usize,
}

impl Budget {
    pub(super) fn reserve(
        &self,
        id: Option<&RequestId>,
        method: &str,
        params: &Value,
    ) -> Result<Reservation, ServerError> {
        let bytes = method.len() + params.to_string().len() + id_size(id);
        self.reserve_bytes(bytes)?;
        Ok(Reservation {
            retained: Arc::clone(&self.0),
            bytes,
        })
    }

    fn reserve_bytes(&self, bytes: usize) -> Result<(), ServerError> {
        let _ = self
            .0
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |held| {
                held.checked_add(bytes)
                    .filter(|sum| *sum <= MAX_RETAINED_BYTES)
            })
            .map_err(|_overflow| ServerError::AnalysisBacklog)?;
        Ok(())
    }
}

fn id_size(id: Option<&RequestId>) -> usize {
    match id {
        Some(RequestId::String(text)) => text.len(),
        Some(RequestId::Number(_)) => size_of::<i64>(),
        None => 0,
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let _ = self.retained.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}
