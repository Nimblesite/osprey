//! The C boundary of an Osprey hosted archive and the host-held slices its
//! runtime reads back ([GPUI-HOST-BOUNDARY], [NATIVE-SHM-EVENT]). Every export is
//! called from one thread: `init`, `argument`, `open` and `event` are serialized
//! on the GPUI main thread, and `native_slice` is answered on that same thread
//! during a call. Nothing is encoded in either direction: arguments are scalars
//! and borrowed strings, and each call's frame is leased from the bridge
//! (`frame.rs`) and checked (`rows.rs`) when the call returns.

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::events::Touch;
use crate::rows::{self, Shown};

extern "C" {
    /// Runs the application's top-level statements and `fn main`; call first.
    fn osprey_main() -> i32;
    /// The application's `argument` export: one launch argument, in order, before `open`.
    fn osprey_app_argument(text: *const c_char) -> i64;
    /// The application's `open` export: draws the first frame.
    fn osprey_app_open(
        now: i64,
        offset: i64,
        platform: *const c_char,
        home: *const c_char,
        dark: bool,
    ) -> i64;
    /// The application's `event` export: delivers one host event and draws the next frame.
    #[allow(clippy::too_many_arguments)]
    fn osprey_app_event(
        now: i64,
        kind: i64,
        ordinal: i64,
        words: *const c_char,
        detail: *const c_char,
        x: f64,
        y: f64,
        down: bool,
        shift: bool,
    ) -> i64;
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
        NO_CALLBACK_FAULT => result,
        OUTSIDE_CALL => Err("Osprey requested an import outside a native call".to_owned()),
        POISONED_STORE => {
            Err("The host slice store was poisoned during an Osprey import".to_owned())
        }
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
        crate::frame::discard();
        callback_result(Ok(()))?;
        if status != 0 {
            return Err(format!("osprey_main returned {status}"));
        }
        INITIALIZED.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Hands the application its launch arguments, in order.
    pub fn arguments(arguments: &[String]) -> Result<(), String> {
        for argument in arguments {
            let text = held(argument)?;
            let _snapshot = ImportSnapshot::begin()?;
            // SAFETY: the argument outlives the call; the export returns a scalar.
            let status = unsafe { osprey_app_argument(text.as_ptr()) };
            sealed(callback_result(Ok(status)), false)?;
        }
        Ok(())
    }

    /// The first frame, for the host's clock, zone offset, platform, home folder and appearance.
    pub fn open(
        now: i64,
        offset: i64,
        platform: &str,
        home: &str,
        dark: bool,
    ) -> Result<Shown, String> {
        let (platform, home) = (held(platform)?, held(home)?);
        let _snapshot = ImportSnapshot::begin()?;
        // SAFETY: both strings outlive the call; the export returns a scalar.
        let status =
            unsafe { osprey_app_open(now, offset, platform.as_ptr(), home.as_ptr(), dark) };
        sealed(callback_result(Ok(status)), false)
    }

    /// The frame after one host event at the clock reading `now`. A refused event still draws the
    /// same view, with the application's reason among the frame's diagnostics.
    pub fn event(now: i64, touch: &Touch) -> Result<Shown, String> {
        let (words, detail) = (held(&touch.words)?, held(&touch.detail)?);
        let _snapshot = ImportSnapshot::begin()?;
        // SAFETY: both strings outlive the call; the export returns a scalar.
        let status = unsafe {
            osprey_app_event(
                now,
                touch.kind,
                touch.ordinal,
                words.as_ptr(),
                detail.as_ptr(),
                touch.x,
                touch.y,
                touch.down,
                touch.shift,
            )
        };
        sealed(callback_result(Ok(status)), true)
    }
}

/// Seals the frame an export wrote once it has returned and checks it ([NATIVE-SHM-SAFETY]). The
/// runs are consumed even when the export failed, and a refused or malformed frame fails the call.
/// Only `event` may answer 1 — a refused event, whose frame still draws with its reason among the
/// diagnostics — and any other answer is a failed export that never becomes a view.
fn sealed(result: Result<i64, String>, event: bool) -> Result<Shown, String> {
    let frame = crate::frame::seal();
    let taken = crate::frame::take();
    let answer = result?;
    frame?;
    if answer != 0 && !(event && answer == 1) {
        return Err(format!("The application refused its call: error {answer}"));
    }
    taken
        .ok_or_else(|| "The bridge published no frame".to_owned())
        .and_then(|frame| rows::read(&frame))
}

#[cfg(test)]
#[path = "ffi_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ffi_test_support.rs"]
pub(crate) mod test_support;
