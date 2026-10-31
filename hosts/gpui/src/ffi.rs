//! The C boundary of an Osprey hosted archive and the host-held slices its
//! runtime reads back ([GPUI-HOST-BOUNDARY]). Every export is called from one
//! thread: `init`, `start` and `dispatch` are serialized on the GPUI main
//! thread, and `native_slice` is answered on that same thread during a call.

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

extern "C" {
    /// Runs the application's top-level statements and `fn main`; call first.
    fn osprey_main() -> i32;
    /// The application's `start` export: one envelope for the launch context.
    fn osprey_app_start(source: *const c_char) -> *const c_char;
    /// The application's `dispatch` export: one envelope for the next event.
    fn osprey_app_dispatch(saved: *const c_char, event: *const c_char) -> *const c_char;
    /// Releases one owned string returned by an application export.
    fn osprey_release_string(value: *const c_char);
    /// ABI 1 copies borrowed import strings before the import adapter returns.
    fn osprey_host_abi_version() -> i32;
}

#[derive(Default)]
struct SliceStore {
    current: HashMap<String, Arc<CString>>,
    snapshot: Option<HashMap<String, Arc<CString>>>,
}

/// Writers update current values; an export retains its immutable read snapshot.
static SLICES: OnceLock<Mutex<SliceStore>> = OnceLock::new();
const NO_CALLBACK_FAULT: u8 = 0;
const OUTSIDE_CALL: u8 = 1;
const POISONED_STORE: u8 = 2;
const POISONED_STREAM: u8 = 3;
const STREAM_FAILURE: &str = "The scene stream storage was poisoned";
static CALLBACK_FAULT: AtomicU8 = AtomicU8::new(NO_CALLBACK_FAULT);

fn slices() -> &'static Mutex<SliceStore> {
    SLICES.get_or_init(|| Mutex::new(SliceStore::default()))
}

struct ImportSnapshot;

impl ImportSnapshot {
    fn begin() -> Result<Self, String> {
        let mut store = slices()
            .lock()
            .map_err(|_| "The host slice store was poisoned".to_owned())?;
        if store.snapshot.is_some() {
            return Err("A native call is already active".to_owned());
        }
        callback_result(Ok(()))?;
        // SAFETY: the ownership marker is a scalar, side-effect-free generated export.
        let version = unsafe { osprey_host_abi_version() };
        if version != 1 {
            return Err(format!(
                "Unsupported Osprey import ownership ABI: {version}"
            ));
        }
        store.snapshot = Some(store.current.clone());
        Ok(Self)
    }
}

impl Drop for ImportSnapshot {
    fn drop(&mut self) {
        let mut store = match slices().lock() {
            Ok(store) => store,
            Err(poisoned) => poisoned.into_inner(),
        };
        store.snapshot = None;
    }
}

fn callback_result<T>(result: Result<T, String>) -> Result<T, String> {
    match CALLBACK_FAULT.swap(NO_CALLBACK_FAULT, Ordering::SeqCst) {
        NO_CALLBACK_FAULT if STREAM.is_poisoned() => Err(STREAM_FAILURE.to_owned()),
        NO_CALLBACK_FAULT => result,
        OUTSIDE_CALL => Err("Osprey requested an import outside a native call".to_owned()),
        POISONED_STORE => {
            Err("The host slice store was poisoned during an Osprey import".to_owned())
        }
        POISONED_STREAM => Err(STREAM_FAILURE.to_owned()),
        _ => Err("Unknown Osprey import callback failure".to_owned()),
    }
}

fn record_callback_fault(code: u8) {
    let _ = CALLBACK_FAULT.compare_exchange(
        NO_CALLBACK_FAULT,
        code,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}

fn failed_import(code: u8) -> *const c_char {
    record_callback_fault(code);
    absent().as_ptr()
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

/// Holds `text` under `key` for the runtime's next `native_slice` fetch.
pub fn put_slice(key: &str, text: &str) -> Result<(), String> {
    let value = Arc::new(held(text)?);
    let _ = slices()
        .lock()
        .map_err(|_| "The host slice store was poisoned".to_owned())?
        .current
        .insert(key.to_owned(), value);
    Ok(())
}

/// Copies host-held slice text back to a synchronous capability adapter.
pub fn get_slice(key: &str) -> Option<String> {
    slices()
        .lock()
        .ok()?
        .current
        .get(key)
        .and_then(|text| text.to_str().ok())
        .map(str::to_owned)
}

/// A borrowed string from this export's snapshot; ABI 1 copies it synchronously.
/// Missing, null and invalid UTF-8 keys return empty. The pointer stays pinned
/// until the enclosing export returns, even while background writers publish.
#[no_mangle]
pub extern "C" fn native_slice(key: *const c_char) -> *const c_char {
    if key.is_null() {
        return absent().as_ptr();
    }
    // SAFETY: the header passes a NUL-terminated string that outlives the call.
    let Ok(name) = unsafe { CStr::from_ptr(key) }.to_str() else {
        return absent().as_ptr();
    };
    match slices().lock() {
        Ok(store) => match &store.snapshot {
            Some(snapshot) => snapshot
                .get(name)
                .map_or_else(|| absent().as_ptr(), |text| text.as_ptr()),
            // Safe C transport only: the Library boundary reports this fault as Err.
            None => failed_import(OUTSIDE_CALL),
        },
        Err(_) => failed_import(POISONED_STORE),
    }
}

/// One rect, disc or sprite a resident application streamed during the current call: the scene
/// encoding's code, a sprite's atlas index, five floats, the packed colour and the depth ([UI-SCENE-STREAM]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Streamed {
    pub code: i64,
    pub index: i64,
    pub values: [f64; 5],
    pub colour: i64,
    pub depth: i64,
}

/// The shapes streamed since the last `start` or `dispatch` began, in the order they arrived.
static STREAM: Mutex<Vec<Streamed>> = Mutex::new(Vec::new());

/// The runtime's scene-stream import ([UI-SCENE-STREAM]): appends one primitive and answers 0.
#[no_mangle]
#[allow(
    clippy::too_many_arguments,
    reason = "the import's C signature is one scalar per primitive field"
)]
pub extern "C" fn native_shape(
    code: i64,
    index: i64,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    colour: i64,
    depth: i64,
) -> i64 {
    push_shape(Streamed {
        code,
        index,
        values: [a, b, c, d, e],
        colour,
        depth,
    });
    0
}

fn push_shape(shape: Streamed) {
    match STREAM.lock() {
        Ok(mut stream) => stream.push(shape),
        Err(_) => record_callback_fault(POISONED_STREAM),
    }
}

/// Takes every shape streamed during the last call, leaving the stream empty.
pub fn take_stream() -> Result<Vec<Streamed>, String> {
    STREAM
        .lock()
        .map(|mut stream| std::mem::take(&mut *stream))
        .map_err(|_| {
            record_callback_fault(POISONED_STREAM);
            STREAM_FAILURE.to_owned()
        })
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// The linked Osprey application: one archive, one calling thread.
pub struct Library;

impl Library {
    /// Validates the ownership boundary on every call and runs `osprey_main` once.
    pub fn init() -> Result<(), String> {
        let _snapshot = ImportSnapshot::begin()?;
        if INITIALIZED.load(Ordering::SeqCst) {
            return Ok(());
        }
        // SAFETY: `osprey_main` takes no arguments and is called before any other export.
        let status = unsafe { osprey_main() };
        callback_result(Ok(()))?;
        if status != 0 {
            return Err(format!("osprey_main returned {status}"));
        }
        INITIALIZED.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// The start envelope for `context`; the caller keeps its `state`.
    pub fn start(context: &str) -> Result<String, String> {
        let source = held(context)?;
        let _snapshot = ImportSnapshot::begin()?;
        let _ = take_stream()?;
        // SAFETY: the argument outlives the call and the result is copied at once.
        unsafe { callback_result(copied(osprey_app_start(source.as_ptr()))) }
    }

    /// The next envelope for the stored `state` and one event.
    pub fn dispatch(state: &str, event: &str) -> Result<String, String> {
        let saved = held(state)?;
        let message = held(event)?;
        let _snapshot = ImportSnapshot::begin()?;
        let _ = take_stream()?;
        // SAFETY: both arguments outlive the call and the result is copied at once.
        unsafe {
            callback_result(copied(osprey_app_dispatch(
                saved.as_ptr(),
                message.as_ptr(),
            )))
        }
    }
}

/// Copies and releases one owned string returned by an application export ([IOS-HOST-ABI]).
fn copied(value: *const c_char) -> Result<String, String> {
    if value.is_null() {
        return Err("The Osprey application returned no envelope".to_owned());
    }
    // SAFETY: a non-null result is a NUL-terminated string valid until released.
    let copied = unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(str::to_owned)
        .map_err(|problem| {
            format!("The Osprey application returned text that is not UTF-8: {problem}")
        });
    // SAFETY: every non-null export result transfers exactly one reference to the host.
    unsafe { osprey_release_string(value) };
    copied
}

#[cfg(test)]
#[path = "ffi_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ffi_test_support.rs"]
pub(crate) mod test_support;
