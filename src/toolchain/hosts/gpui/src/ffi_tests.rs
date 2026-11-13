use super::test_support::{isolated_test, poison, serial, LIVE_BUFFERS, TEXT_BYTES};
use super::*;
use crate::frame;
use crate::rows::Kind;
use crate::scene::Shape;
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

/// A frame every check accepts: one canvas and one lettered scene.
fn healthy_frame() -> i64 {
    let canvas = view(CANVAS, -1, 0, "", 0, "");
    assert_eq!(canvas, 0, "the first control row");
    scene_head(canvas);
    assert_eq!(lettering("Count 0"), 0);
    0
}

/// A frame a scene row writes no canvas can name.
fn malformed_frame() -> i64 {
    let _ = view(LABEL, -1, -1, "stray", 0, "");
    let _ = lettering("before any scene");
    0
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

/// Writes the frame this test mode asks for and returns the export's status.
fn output() -> i64 {
    EXPORTS.fetch_add(1, Ordering::SeqCst);
    match MODE.load(Ordering::SeqCst) {
        0 => 0,
        1 => healthy_frame(),
        2 => malformed_frame(),
        3 => observe(snapshot_observation),
        4 => nested_output(),
        5 => observe(bounded_observation),
        6 => {
            *snapshot_slot() = Some(empty_key_observation());
            0
        }
        7 => nested_dispatch_output(),
        8 | 9 => poisoned_callback_output(),
        10 | 12 => refused_frame_output(),
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

#[test]
fn a_checked_frame_survives_its_lease_and_is_read_once() {
    let _serial = serial();
    reset(1);
    let shown = Library::open(0, 0, "test", "/home", false).expect("a checked frame");
    assert!(
        frame::take().is_none(),
        "the checked frame was left pending"
    );
    assert_eq!(shown.controls.len(), 1);
    assert_eq!(shown.controls[0].kind, Kind::Canvas);
    let scene = shown.scenes.get(&0).expect("the canvas's scene");
    assert!(
        scene
            .shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Lettering { text, .. } if text == "Count 0")),
        "the scene text did not survive its lease: {scene:?}"
    );
}

#[test]
fn a_malformed_frame_is_refused_and_consumed() {
    let _serial = serial();
    reset(2);
    let result = Library::open(0, 0, "test", "/home", false);
    assert!(
        result.is_err(),
        "a malformed frame was accepted: {result:?}"
    );
    assert!(
        frame::take().is_none(),
        "the malformed frame was left pending"
    );
    MODE.store(1, Ordering::SeqCst);
    assert_eq!(
        Library::open(0, 0, "test", "/home", false).map(|shown| shown.controls.len()),
        Ok(1)
    );
}

#[test]
fn a_call_that_streams_nothing_reads_an_empty_view() {
    let _serial = serial();
    reset(0);
    let shown = Library::open(0, 0, "test", "/home", false).expect("an empty view");
    assert!(shown.controls.is_empty(), "an empty frame drew controls");
    assert!(shown.roots.is_empty(), "an empty frame drew roots");
    assert!(shown.asks.is_empty(), "an empty frame asked for work");
    assert!(frame::take().is_none(), "the empty frame was left pending");
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

fn prepare_snapshot() -> std::sync::MutexGuard<'static, ()> {
    let guard = serial();
    reset(3);
    assert_eq!(put_slice("snapshot-test", "before"), Ok(()));
    guard
}

#[test]
fn open_pins_imports_across_a_concurrent_slice_replacement() {
    let _serial = prepare_snapshot();
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some("before|before|after"));
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some("after|after|after"));
}

#[test]
fn event_pins_imports_across_a_concurrent_slice_replacement() {
    let _serial = prepare_snapshot();
    assert!(Library::event(0, &Touch::control(0, "")).is_ok());
    assert_eq!(snapshot().as_deref(), Some("before|before|after"));
}

#[test]
fn initialization_pins_imports_and_releases_a_failed_call_scope() {
    let _serial = prepare_snapshot();
    MAIN_STATUS.store(7, Ordering::SeqCst);
    assert!(Library::init().is_err());
    assert_eq!(main_observation().as_deref(), Some("before|before|after"));
    MAIN_STATUS.store(0, Ordering::SeqCst);
    assert_eq!(Library::init(), Ok(()));
    assert_eq!(main_observation().as_deref(), Some("after|after|after"));
}

#[test]
fn incompatible_import_ownership_versions_are_refused_before_exports() {
    let _serial = serial();
    for version in [0, 2] {
        reset(1);
        ABI.store(version, Ordering::SeqCst);
        assert!(Library::init().is_err(), "unsupported ABI initialized");
        assert!(
            Library::open(0, 0, "test", "/home", false).is_err(),
            "unsupported ABI opened"
        );
        assert!(
            Library::event(0, &Touch::control(0, "")).is_err(),
            "unsupported ABI dispatched"
        );
        assert!(
            Library::arguments(&["given".to_owned()]).is_err(),
            "unsupported ABI argued"
        );
        assert_eq!(EXPORTS.load(Ordering::SeqCst), 0, "an export ran");
    }
}

#[test]
fn malformed_output_does_not_leave_an_import_snapshot_active() {
    let _serial = prepare_snapshot();
    MODE.store(2, Ordering::SeqCst);
    assert!(Library::open(0, 0, "test", "/home", false).is_err());
    MODE.store(3, Ordering::SeqCst);
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some("before|before|after"));
}

#[test]
fn a_nested_export_is_refused_without_disturbing_the_outer_scope() {
    let _serial = prepare_snapshot();
    MODE.store(4, Ordering::SeqCst);
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(
        snapshot().as_deref(),
        Some("A native call is already active")
    );
    MODE.store(3, Ordering::SeqCst);
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some("before|before|after"));
}

#[test]
fn nul_input_does_not_leave_an_import_snapshot_active() {
    let _serial = prepare_snapshot();
    assert!(Library::open(0, 0, "bad\0folder", "/home", false).is_err());
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some("before|before|after"));
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

#[test]
fn allocator_control_observes_live_slice_buffers_and_their_release() {
    let _serial = serial();
    let before = LIVE_BUFFERS.load(Ordering::SeqCst);
    let value = CString::new("p".repeat(TEXT_BYTES)).expect("valid probe");
    assert_eq!(
        LIVE_BUFFERS.load(Ordering::SeqCst),
        before + 1,
        "allocator control inactive"
    );
    drop(value);
    assert_eq!(
        LIVE_BUFFERS.load(Ordering::SeqCst),
        before,
        "deallocation control inactive"
    );
}

#[test]
fn concurrent_replacements_retain_only_the_current_buffer_and_active_reader() {
    let _serial = serial();
    reset(5);
    assert_eq!(Library::init(), Ok(()));
    let before = LIVE_BUFFERS.load(Ordering::SeqCst);
    let original = "s".repeat(TEXT_BYTES);
    assert_eq!(put_slice("allocation-test", &original), Ok(()));
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some(original.as_str()));
    assert_eq!(
        PINNED_BUFFERS.load(Ordering::SeqCst) - before,
        2,
        "only current value and active reader may remain"
    );
    assert_eq!(
        LIVE_BUFFERS.load(Ordering::SeqCst) - before,
        1,
        "reader pin must drop after export"
    );
    assert_eq!(get_slice("allocation-test"), Some("w".repeat(TEXT_BYTES)));
}

fn empty_key_observation() -> String {
    let invalid = [0xff_u8, 0];
    let missing = held("missing-import-key").expect("valid key");
    let pointers = [
        native_slice(std::ptr::null()),
        native_slice(invalid.as_ptr().cast()),
        native_slice(missing.as_ptr()),
    ];
    let texts: Result<Vec<_>, _> = pointers.into_iter().map(import_text).collect();
    format!("{texts:?}")
}

#[test]
fn missing_null_and_invalid_keys_remain_empty_inside_a_call() {
    let _serial = serial();
    reset(6);
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some("Ok([\"\", \"\", \"\"])"));
}

#[test]
fn a_rust_unwind_releases_the_import_scope() {
    let _serial = serial();
    reset(3);
    assert_eq!(put_slice("snapshot-test", "before"), Ok(()));
    let result = std::panic::catch_unwind(|| {
        let _snapshot = ImportSnapshot::begin().expect("supported ABI and idle scope");
        panic!("unwind cleanup control");
    });
    assert!(result.is_err());
    MODE.store(1, Ordering::SeqCst);
    let shown = Library::open(0, 0, "test", "/home", false).expect("the next call's frame");
    assert_eq!(shown.controls.len(), 1);
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

#[test]
fn refused_nested_dispatch_preserves_the_outer_calls_streamed_shapes() {
    let _serial = serial();
    reset(7);
    let shown = Library::open(0, 0, "test", "/home", false).expect("the outer call's frame");
    assert_eq!(snapshot().as_deref(), Some("true,true,true"));
    let scene = shown.scenes.get(&0).expect("the outer frame's scene");
    assert_eq!(scene.shapes.len(), 3, "the nested calls consumed rows");
}

#[test]
fn an_import_outside_a_call_is_reported_before_any_export_runs() {
    let _serial = serial();
    reset(1);
    let key = held("outside-call").expect("valid key");
    assert_eq!(import_text(native_slice(key.as_ptr())), Ok(String::new()));
    assert!(
        Library::open(0, 0, "test", "/home", false).is_err(),
        "the callback fault became a successful view"
    );
    assert_eq!(EXPORTS.load(Ordering::SeqCst), 0, "an export ran");
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

#[test]
fn a_poisoned_import_reports_failure_and_discards_the_export_result() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(8);
    MODE.store(1, Ordering::SeqCst);
    let shown = Library::open(0, 0, "test", "/home", false).expect("the next call's frame");
    assert_eq!(shown.controls.len(), 1);
}

#[test]
fn a_refused_nested_call_cannot_consume_the_outer_import_fault() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(9);
}

/// A failed export leaves no frame, so it cannot become a successful view.
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

/// One scene row that no scene head names.
fn marker() -> i64 {
    // SAFETY: the writer takes no text and returns whether the row was accepted.
    unsafe { osp_native_scene_shape(934, 0, 0., 0., 0., 0., 0., 0, 0, std::ptr::null()) }
}

/// Mode 10 streams one shape more than a frame may hold; mode 12 streams two inside a frame
/// whose control no check accepts.
fn refused_frame_output() -> i64 {
    if MODE.load(Ordering::SeqCst) == 12 {
        let _ = view(99, -1, -1, "unknown kind", 0, "");
        assert_eq!(lettering("one"), 0);
        assert_eq!(lettering("two"), 0);
        return 0;
    }
    let accepted = (0..=1 << 20).filter(|_| marker() == 0).count();
    assert_eq!(accepted, 1 << 20, "the frame's shape limit moved");
    0
}

/// The next export after a failed one succeeds and its frame holds none of the failed call's shapes.
fn assert_clean_frame() {
    assert!(frame::take().is_none(), "a failed export left a frame");
    MODE.store(1, Ordering::SeqCst);
    let shown = Library::open(0, 0, "test", "/home", false).expect("the healthy call's frame");
    assert_eq!(shown.controls.len(), 1, "{shown:?}");
    let scene = shown.scenes.get(&0).expect("the healthy frame's scene");
    assert_eq!(scene.shapes.len(), 1, "{scene:?}");
    assert!(
        frame::take().is_none(),
        "the healthy frame was left pending"
    );
}

#[test]
fn a_refused_frame_cannot_become_a_successful_view() {
    let _serial = assert_import_failure(10);
    assert_clean_frame();
}

#[test]
fn shapes_streamed_by_a_failed_export_do_not_leak_into_the_next_frame() {
    let _serial = serial();
    reset(12);
    assert!(Library::open(0, 0, "test", "/home", false).is_err());
    assert!(frame::take().is_none());
    assert_clean_frame();
}
