//! One callback per concern, safe to invoke from inside itself.
//!
//! GPUI re-registers window callbacks while earlier ones run (for example a frame
//! callback that triggers focus handling). A [`Slot`] withdraws its callback for
//! the duration of [`Slot::with`], so a handler may replace itself, withdraw
//! itself, or re-enter the same slot without aliasing the callback.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// A single optional callback, replaced by [`Slot::set`] and invoked by [`Slot::with`].
pub(crate) struct Slot<T: ?Sized> {
    callback: Mutex<Option<Box<T>>>,
}

impl<T: ?Sized> Default for Slot<T> {
    fn default() -> Self {
        Self {
            callback: Mutex::new(None),
        }
    }
}

impl<T: ?Sized> Slot<T> {
    /// Installs `callback`, dropping any previous one.
    pub fn set(&self, callback: Box<T>) {
        *locked(&self.callback) = Some(callback);
    }

    /// Removes and returns the callback, leaving the slot empty.
    pub fn take(&self) -> Option<Box<T>> {
        locked(&self.callback).take()
    }

    /// Runs the installed callback with it withdrawn, restoring it afterwards
    /// unless the handler installed a replacement or withdrew itself.
    pub fn with<R>(&self, run: impl FnOnce(&mut T) -> R) -> Option<R> {
        let mut callback = self.take()?;
        let result = run(&mut *callback);
        let mut guard = locked(&self.callback);
        if guard.is_none() {
            *guard = Some(callback);
        }
        Some(result)
    }
}

/// Locks without panicking on a poisoned mutex: the callback state stays usable
/// after a handler panicked, which is the only way to see the next frame's diagnostics.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
