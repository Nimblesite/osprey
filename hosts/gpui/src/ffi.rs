//! The C boundary of an Osprey macOS archive and the host-held slices its
//! runtime reads back ([GPUI-HOST-BOUNDARY]). Every export is called from one
//! thread: `init`, `start` and `dispatch` are serialized on the GPUI main
//! thread, and `gpui_slice` is answered on that same thread during a call.

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

extern "C" {
    /// Runs the application's top-level statements and `fn main`; call first.
    fn osprey_main() -> i32;
    /// The application's `start` export: one envelope for the launch context.
    fn osprey_app_start(source: *const c_char) -> *const c_char;
    /// The application's `dispatch` export: one envelope for the next event.
    fn osprey_app_dispatch(saved: *const c_char, event: *const c_char) -> *const c_char;
}

/// The text the runtime fetches through `gpui_slice`, by slice key.
static SLICES: OnceLock<Mutex<HashMap<String, CString>>> = OnceLock::new();

/// Buffers this process already handed to the runtime. Nothing is ever freed
/// from here, so a pointer answered once stays valid for the rest of the process.
static RETIRED: OnceLock<Mutex<Vec<CString>>> = OnceLock::new();

fn slices() -> &'static Mutex<HashMap<String, CString>> {
    SLICES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn retired() -> &'static Mutex<Vec<CString>> {
    RETIRED.get_or_init(|| Mutex::new(Vec::new()))
}

/// The answer for a key the host never wrote: an empty string, never a null.
fn absent() -> &'static CString {
    static EMPTY: OnceLock<CString> = OnceLock::new();
    EMPTY.get_or_init(CString::default)
}

/// A C string for a call; text with a NUL byte cannot cross the boundary at all.
fn held(text: &str) -> Result<CString, String> {
    CString::new(text)
        .map_err(|_| "Host text contained a NUL byte, which no C string can carry".to_owned())
}

/// Holds `text` under `key` for the runtime's next `gpui_slice` fetch.
pub fn put_slice(key: &str, text: &str) -> Result<(), String> {
    let value = held(text)?;
    let replaced = slices()
        .lock()
        .map_err(|_| "The host slice store was poisoned".to_owned())?
        .insert(key.to_owned(), value);
    if let Some(previous) = replaced {
        match retired().lock() {
            Ok(mut kept) => kept.push(previous),
            // A buffer that cannot be parked is leaked, never freed.
            Err(_) => std::mem::forget(previous),
        }
    }
    Ok(())
}

/// Copies host-held slice text back to a synchronous capability adapter.
pub fn get_slice(key: &str) -> Option<String> {
    slices()
        .lock()
        .ok()?
        .get(key)
        .and_then(|text| text.to_str().ok())
        .map(str::to_owned)
}

/// The runtime's `ui::Slices` import: the text last written under `key`, or the
/// empty string when the host holds none. `key` must be null or a NUL-terminated
/// C string valid for the call, as the generated header states.
#[no_mangle]
pub extern "C" fn gpui_slice(key: *const c_char) -> *const c_char {
    if key.is_null() {
        return absent().as_ptr();
    }
    // SAFETY: the header passes a NUL-terminated string that outlives the call.
    let asked = unsafe { CStr::from_ptr(key) };
    let Ok(name) = asked.to_str() else {
        return absent().as_ptr();
    };
    match slices().lock() {
        Ok(store) => store
            .get(name)
            .map_or_else(|| absent().as_ptr(), |text| text.as_ptr()),
        Err(_) => absent().as_ptr(),
    }
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// The linked Osprey application: one archive, one calling thread.
pub struct Library;

impl Library {
    /// Runs `osprey_main` once, before any other export; later calls are no-ops.
    pub fn init() -> Result<(), String> {
        if INITIALIZED.load(Ordering::SeqCst) {
            return Ok(());
        }
        // SAFETY: `osprey_main` takes no arguments and is called before any other export.
        let status = unsafe { osprey_main() };
        if status != 0 {
            return Err(format!("osprey_main returned {status}"));
        }
        INITIALIZED.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// The start envelope for `context`; the caller keeps its `state`.
    pub fn start(context: &str) -> Result<String, String> {
        let source = held(context)?;
        // SAFETY: the argument outlives the call and the result is copied at once.
        unsafe { copied(osprey_app_start(source.as_ptr())) }
    }

    /// The next envelope for the stored `state` and one event.
    pub fn dispatch(state: &str, event: &str) -> Result<String, String> {
        let saved = held(state)?;
        let message = held(event)?;
        // SAFETY: both arguments outlive the call and the result is copied at once.
        unsafe { copied(osprey_app_dispatch(saved.as_ptr(), message.as_ptr())) }
    }
}

/// Copies a returned string: it belongs to Osprey and is overwritten by the next call.
fn copied(value: *const c_char) -> Result<String, String> {
    if value.is_null() {
        return Err("The Osprey application returned no envelope".to_owned());
    }
    // SAFETY: a non-null result is a NUL-terminated string valid until the next call.
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(str::to_owned)
        .map_err(|problem| {
            format!("The Osprey application returned text that is not UTF-8: {problem}")
        })
}
