//! The fixture every FFI test drives [GPUI-HOST-BOUNDARY], [NATIVE-SHM-EVENT]: one linked
//! application whose exports stream rows through the real bridge, answer the statuses a host must
//! refuse, and read host-held slices through `native_slice`. The pinning tests of the slice store
//! live in `ffi_import_tests.rs` and the frame's own tests in `ffi_frame_tests.rs`.

use super::test_support::{poison, serial, LIVE_BUFFERS, TEXT_BYTES};
use super::*;
use crate::frame;
use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicUsize};

static MODE: AtomicUsize = AtomicUsize::new(0);
static EXPORTS: AtomicUsize = AtomicUsize::new(0);
static ABI: AtomicI32 = AtomicI32::new(1);
static MAIN_STATUS: AtomicI32 = AtomicI32::new(0);
static MAIN_OUTPUT: Mutex<Option<String>> = Mutex::new(None);
static SNAPSHOT: Mutex<Option<String>> = Mutex::new(None);
static PINNED_BUFFERS: AtomicIsize = AtomicIsize::new(0);

/// The row kind of a canvas, whose scene rows name its row ([NATIVE-SHM-VIEW]).
const CANVAS: i64 = 13;
/// The row kind of a text label.
const LABEL: i64 = 2;
/// The font weight every test control carries.
const WEIGHT: i64 = 400;
/// The ask code of a refused event's diagnostic ([NATIVE-SHM-ASK]).
const DIAGNOSTIC: i64 = 101;
/// The answer of an export that failed: no view may be read from it.
const EXPORT_FAILED: i64 = 7;
/// The most scene rows and view rows one frame may hold ([NATIVE-SHM]).
const SHAPE_ROWS: i64 = 1 << 20;
const VIEW_ROWS: i64 = 1 << 16;

extern "C" {
    /// The runtime's control writer, which copies its text before returning.
    fn osp_native_view_row(
        kind: i64,
        parent: i64,
        action: i64,
        text: *const c_char,
        colour: i64,
        background: i64,
        size: f64,
        padding: f64,
        spacing: f64,
        radius: f64,
        weight: i64,
    ) -> i64;
    /// The runtime's control fitting writer.
    fn osp_native_view_extend(
        row: i64,
        width: f64,
        height: f64,
        grow: f64,
        flags: i64,
        border: i64,
        hint: *const c_char,
    ) -> i64;
    /// The runtime's scene row writer, which copies its text before returning.
    fn osp_native_scene_shape(
        code: i64,
        index: i64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        angle: f64,
        colour: i64,
        depth: i64,
        text: *const c_char,
    ) -> i64;
    /// The runtime's ask writer, which copies its text before returning.
    fn osp_native_ask(field: i64, number: i64, text: *const c_char) -> i64;
}

fn reset(mode: usize) {
    MODE.store(mode, Ordering::SeqCst);
    EXPORTS.store(0, Ordering::SeqCst);
    ABI.store(1, Ordering::SeqCst);
    MAIN_STATUS.store(0, Ordering::SeqCst);
    INITIALIZED.store(false, Ordering::SeqCst);
    CALLBACK_FAULT.store(NO_CALLBACK_FAULT, Ordering::SeqCst);
    *main_slot() = None;
    *snapshot_slot() = None;
}

/// One C string a writer copies before returning.
fn text(value: &str) -> CString {
    CString::new(value).expect("test text must not contain an interior NUL")
}

/// Streams one control and its fit, returning the row its children name.
fn view(kind: i64, parent: i64, action: i64, words: &str, traits: i64, hint: &str) -> i64 {
    let words = text(words);
    // SAFETY: both writers copy their text before returning, and the pointers outlive the calls.
    let row = unsafe {
        osp_native_view_row(
            kind,
            parent,
            action,
            words.as_ptr(),
            0,
            0,
            0.,
            0.,
            0.,
            0.,
            WEIGHT,
        )
    };
    let hint = text(hint);
    // SAFETY: `row` is the control just written; the writer copies its text before returning.
    let fitted = unsafe { osp_native_view_extend(row, 0., 0., 0., traits, 0, hint.as_ptr()) };
    assert_eq!(fitted, 0, "a test control must accept its fit");
    row
}

/// Streams the scene head naming the canvas at `canvas`: its first three values are the scene's
/// width, height and zoom.
fn scene_head(canvas: i64) {
    // SAFETY: the writer copies its text before returning.
    let row = unsafe {
        osp_native_scene_shape(20, canvas, 100., 100., 1., 0., 0., 0, 0, text("").as_ptr())
    };
    assert_eq!(row, 0, "a test scene head must be written");
}

/// Streams one lettering shape into the current scene.
fn lettering(drawn: &str) -> i64 {
    // SAFETY: the writer copies its text before returning.
    unsafe { osp_native_scene_shape(6, 0, 0., 0., 12., 400., 0., 0, 0, text(drawn).as_ptr()) }
}

/// Streams one lettering shape carrying raw bytes, so a frame can carry text that is not UTF-8.
fn raw_lettering(bytes: &[u8]) -> i64 {
    // SAFETY: no byte is a NUL, so the C string is well formed for the boundary.
    let words = unsafe { CString::from_vec_unchecked(bytes.to_vec()) };
    // SAFETY: the writer copies its text before returning.
    unsafe { osp_native_scene_shape(6, 0, 0., 0., 12., 400., 0., 0, 0, words.as_ptr()) }
}

/// A frame every check accepts: one canvas and one lettered scene counting `count`.
fn counted_frame(count: &str) -> i64 {
    let canvas = view(CANVAS, -1, 0, "", 0, "");
    assert_eq!(canvas, 0, "the first control row");
    scene_head(canvas);
    assert_eq!(lettering(count), 0);
    0
}

/// A frame every check accepts: one canvas and one lettered scene.
fn healthy_frame() -> i64 {
    counted_frame("Count 0")
}

/// A frame a scene row writes no canvas can name.
fn malformed_frame() -> i64 {
    let _ = view(LABEL, -1, -1, "stray", 0, "");
    let _ = lettering("before any scene");
    0
}

/// A label whose text is not UTF-8: the bridge stores the bytes, and the check refuses them.
fn malformed_label_frame() -> i64 {
    // SAFETY: no byte is a NUL, so the C string is well formed for the boundary.
    let words = unsafe { CString::from_vec_unchecked(vec![0xff, b'x']) };
    // SAFETY: the writer copies its text before returning.
    let row =
        unsafe { osp_native_view_row(LABEL, -1, -1, words.as_ptr(), 0, 0, 0., 0., 0., 0., WEIGHT) };
    assert_eq!(row, 0, "the malformed label must still be written");
    0
}

/// A lettering whose text is not UTF-8, drawn into an otherwise healthy scene.
fn malformed_scene_frame() -> i64 {
    let canvas = view(CANVAS, -1, 0, "", 0, "");
    scene_head(canvas);
    assert_eq!(
        raw_lettering(&[0xff, b'x']),
        0,
        "the malformed row must be written"
    );
    0
}

/// A healthy frame answered with a failure: the answer alone must refuse the call.
fn failed_answer_frame() -> i64 {
    assert_eq!(healthy_frame(), 0, "the failed export still drew a frame");
    EXPORT_FAILED
}

/// The frame a refused event draws: the same view again, its reason among the asks.
fn refused_event_frame() -> i64 {
    assert_eq!(healthy_frame(), 0, "the refused event still drew its frame");
    // SAFETY: the writer copies its text before returning.
    let asked = unsafe { osp_native_ask(0, DIAGNOSTIC, text("the picker was closed").as_ptr()) };
    assert_eq!(asked, 0, "the diagnostic ask must be written");
    1
}

/// A strip that claims two point rows which never follow: written and sealed, refused at the read.
fn unreadable_scene_frame() -> i64 {
    let canvas = view(CANVAS, -1, 0, "", 0, "");
    scene_head(canvas);
    // SAFETY: the writer takes no text and returns whether the row was accepted.
    let strip = unsafe { osp_native_scene_shape(2, 2, 0., 0., 0., 0., 0., 0, 0, std::ptr::null()) };
    assert_eq!(strip, 0, "the unreadable strip must still be written");
    0
}

/// An ask for a capability no host answers: written and sealed, refused at the read.
fn unreadable_ask_frame() -> i64 {
    // SAFETY: the writer copies its text before returning.
    let asked = unsafe { osp_native_ask(0, 26, text("no such capability").as_ptr()) };
    assert_eq!(asked, 0, "the unknown ask must still be written");
    0
}

/// Writes the frame this test mode asks for and returns the export's answer.
fn output() -> i64 {
    EXPORTS.fetch_add(1, Ordering::SeqCst);
    match MODE.load(Ordering::SeqCst) {
        0 => 0,
        1 => counted_frame("Count 0"),
        2 => malformed_frame(),
        3 => observe(snapshot_observation),
        4 => nested_output(),
        5 => observe(bounded_observation),
        6 => observe(empty_key_observation),
        7 => nested_dispatch_output(),
        8 | 9 => poisoned_callback_output(),
        10 | 12 | 18 => refused_frame_output(),
        mode => framed_output(mode),
    }
}

/// The modes that draw one whole frame and answer it: a second healthy count, text no check
/// accepts, an answer a host must refuse, and rows the write accepts but the read does not.
fn framed_output(mode: usize) -> i64 {
    match mode {
        13 => counted_frame("Count 1"),
        14 => malformed_label_frame(),
        15 => malformed_scene_frame(),
        16 => failed_answer_frame(),
        17 => refused_event_frame(),
        19 => unreadable_scene_frame(),
        20 => unreadable_ask_frame(),
        mode => unreachable!("unknown test output mode {mode}"),
    }
}

#[no_mangle]
extern "C" fn osprey_main() -> i32 {
    EXPORTS.fetch_add(1, Ordering::SeqCst);
    if MODE.load(Ordering::SeqCst) == 3 {
        *main_slot() = Some(snapshot_observation().unwrap_or_else(|error| error));
    }
    MAIN_STATUS.load(Ordering::SeqCst)
}

#[no_mangle]
extern "C" fn osprey_host_abi_version() -> i32 {
    ABI.load(Ordering::SeqCst)
}

#[no_mangle]
extern "C" fn osprey_app_argument(_text: *const c_char) -> i64 {
    output()
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
extern "C" fn osprey_app_open(
    _now: i64,
    _offset: i64,
    _platform: *const c_char,
    _home: *const c_char,
    _dark: bool,
) -> i64 {
    output()
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
extern "C" fn osprey_app_event(
    _now: i64,
    _kind: i64,
    _ordinal: i64,
    _words: *const c_char,
    _detail: *const c_char,
    _x: f64,
    _y: f64,
    _down: bool,
    _shift: bool,
) -> i64 {
    output()
}

fn main_slot() -> std::sync::MutexGuard<'static, Option<String>> {
    MAIN_OUTPUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn snapshot_slot() -> std::sync::MutexGuard<'static, Option<String>> {
    SNAPSHOT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn snapshot() -> Option<String> {
    snapshot_slot().clone()
}

fn main_observation() -> Option<String> {
    main_slot().clone()
}

/// Records what an export observed, since an export now returns a status rather than text.
fn observe(observation: fn() -> Result<String, String>) -> i64 {
    *snapshot_slot() = Some(observation().unwrap_or_else(|error| error));
    0
}

fn import_text(pointer: *const c_char) -> Result<String, String> {
    // SAFETY: the import's buffer remains pinned until its enclosing export returns.
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map(str::to_owned)
        .map_err(|error| error.to_string())
}

fn snapshot_observation() -> Result<String, String> {
    let key = held("snapshot-test")?;
    let pointer = native_slice(key.as_ptr());
    std::thread::spawn(|| put_slice("snapshot-test", "after"))
        .join()
        .map_err(|_| "slice writer panicked".to_owned())??;
    let first = import_text(pointer)?;
    let again = import_text(native_slice(key.as_ptr()))?;
    let latest = get_slice("snapshot-test").ok_or("missing latest slice")?;
    Ok(format!("{first}|{again}|{latest}"))
}

fn nested_output() -> i64 {
    MODE.store(0, Ordering::SeqCst);
    let problem = Library::open(0, 0, "nested", "/home", false)
        .err()
        .unwrap_or_else(|| "accepted nested call".to_owned());
    MODE.store(4, Ordering::SeqCst);
    *snapshot_slot() = Some(problem);
    0
}

fn bounded_observation() -> Result<String, String> {
    let key = held("allocation-test")?;
    let pointer = native_slice(key.as_ptr());
    std::thread::spawn(|| {
        let replacement = "w".repeat(TEXT_BYTES);
        (0..256).try_for_each(|_| put_slice("allocation-test", &replacement))
    })
    .join()
    .map_err(|_| "slice writer panicked".to_owned())??;
    let copied = import_text(pointer)?;
    PINNED_BUFFERS.store(LIVE_BUFFERS.load(Ordering::SeqCst), Ordering::SeqCst);
    Ok(copied)
}

fn empty_key_observation() -> Result<String, String> {
    let invalid = [0xff_u8, 0];
    let missing = held("missing-import-key").expect("valid key");
    let pointers = [
        native_slice(std::ptr::null()),
        native_slice(invalid.as_ptr().cast()),
        native_slice(missing.as_ptr()),
    ];
    let texts: Result<Vec<_>, _> = pointers.into_iter().map(import_text).collect();
    Ok(format!("{texts:?}"))
}

fn nested_dispatch_output() -> i64 {
    let canvas = view(CANVAS, -1, 0, "", 0, "");
    scene_head(canvas);
    let observations = [("event", 1), ("bad\0event", 1), ("event", 2)].map(|(event, abi)| {
        assert_eq!(lettering("shape"), 0);
        ABI.store(abi, Ordering::SeqCst);
        MODE.store(0, Ordering::SeqCst);
        let refused = Library::event(0, &Touch::control(0, event)).is_err();
        MODE.store(7, Ordering::SeqCst);
        ABI.store(1, Ordering::SeqCst);
        refused.to_string()
    });
    *snapshot_slot() = Some(observations.join(","));
    0
}

fn poisoned_callback_output() -> i64 {
    poison(slices());
    let key = held("snapshot-test").expect("valid key");
    let _ = native_slice(key.as_ptr());
    if MODE.load(Ordering::SeqCst) == 9 {
        let _ = Library::open(0, 0, "nested", "/home", false);
    }
    0
}

/// One scene row that no scene head names.
fn marker() -> i64 {
    // SAFETY: the writer takes no text and returns whether the row was accepted.
    unsafe { osp_native_scene_shape(934, 0, 0., 0., 0., 0., 0., 0, 0, std::ptr::null()) }
}

/// Mode 10 streams one shape row more than a frame may hold and mode 18 one view row more, so a
/// run refuses and the frame cannot publish; mode 12 streams rows no check accepts.
fn refused_frame_output() -> i64 {
    match MODE.load(Ordering::SeqCst) {
        12 => {
            let _ = view(99, -1, -1, "unknown kind", 0, "");
            assert_eq!(lettering("one"), 0);
            assert_eq!(lettering("two"), 0);
            0
        }
        18 => {
            let accepted = refused_view_writes();
            assert_eq!(accepted, VIEW_ROWS, "the frame's view limit moved");
            0
        }
        _ => {
            let accepted = (0..=SHAPE_ROWS).filter(|_| marker() == 0).count();
            assert_eq!(accepted as i64, SHAPE_ROWS, "the frame's shape limit moved");
            0
        }
    }
}

/// Streams label rows until the view run refuses, returning how many it accepted.
fn refused_view_writes() -> i64 {
    (0..=VIEW_ROWS)
        .filter(|at| {
            // SAFETY: the writer takes no text and returns the row it accepted, or -1.
            let written = unsafe {
                osp_native_view_row(
                    LABEL,
                    -1,
                    -1,
                    std::ptr::null(),
                    0,
                    0,
                    0.,
                    0.,
                    0.,
                    0.,
                    WEIGHT,
                )
            };
            written == *at
        })
        .count() as i64
}

/// A failed export leaves no frame, so it cannot become a successful view; the runs are consumed.
fn assert_import_failure(mode: usize) -> std::sync::MutexGuard<'static, ()> {
    let guard = serial();
    reset(mode);
    let result = Library::open(0, 0, "test", "/home", false);
    slices().clear_poison();
    assert!(
        result.is_err(),
        "import failure {mode} became a successful view"
    );
    assert!(
        frame::take().is_none(),
        "import failure {mode} left a frame"
    );
    guard
}

#[path = "ffi_import_tests.rs"]
mod import_tests;

#[path = "ffi_frame_tests.rs"]
mod frame_tests;
