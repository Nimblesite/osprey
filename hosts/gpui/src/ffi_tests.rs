use super::test_support::{isolated_test, poison, serial, LIVE_BUFFERS, TEXT_BYTES};
use super::*;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::AtomicUsize;

static MODE: AtomicUsize = AtomicUsize::new(0);
static RELEASES: AtomicUsize = AtomicUsize::new(0);
static ABI: AtomicI32 = AtomicI32::new(1);
static MAIN_STATUS: AtomicI32 = AtomicI32::new(0);
static MAIN_OUTPUT: Mutex<Option<String>> = Mutex::new(None);
static PINNED_BUFFERS: AtomicIsize = AtomicIsize::new(0);

fn reset(mode: usize) {
    MODE.store(mode, Ordering::SeqCst);
    RELEASES.store(0, Ordering::SeqCst);
    ABI.store(1, Ordering::SeqCst);
    MAIN_STATUS.store(0, Ordering::SeqCst);
    INITIALIZED.store(false, Ordering::SeqCst);
    CALLBACK_FAULT.store(NO_CALLBACK_FAULT, Ordering::SeqCst);
}

fn owned(bytes: &[u8]) -> *const c_char {
    CString::new(bytes)
        .expect("test output must not contain an interior NUL")
        .into_raw()
}

fn output() -> *const c_char {
    match MODE.load(Ordering::SeqCst) {
        0 => std::ptr::null(),
        1 => owned(b"copied envelope"),
        2 => owned(&[0xff, b'x']),
        3 => observation_output(),
        4 => nested_output(),
        5 => allocation_output(),
        6 => empty_key_output(),
        7 => nested_dispatch_output(),
        8 => poisoned_callback_output(),
        9 => poisoned_callback_output(),
        10..=12 => poisoned_stream_output(),
        mode => unreachable!("unknown test output mode {mode}"),
    }
}

#[no_mangle]
extern "C" fn osprey_main() -> i32 {
    if MODE.load(Ordering::SeqCst) == 3 {
        *MAIN_OUTPUT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(snapshot_observation().unwrap_or_else(|error| error));
    }
    MAIN_STATUS.load(Ordering::SeqCst)
}

#[no_mangle]
extern "C" fn osprey_host_abi_version() -> i32 {
    ABI.load(Ordering::SeqCst)
}

#[no_mangle]
extern "C" fn osprey_app_start(_source: *const c_char) -> *const c_char {
    output()
}

#[no_mangle]
extern "C" fn osprey_app_dispatch(_saved: *const c_char, _event: *const c_char) -> *const c_char {
    output()
}

#[no_mangle]
extern "C" fn osprey_release_string(value: *const c_char) {
    assert!(!value.is_null(), "the host released a null output");
    RELEASES.fetch_add(1, Ordering::SeqCst);
    // SAFETY: each test output is a live CString allocation released at most once.
    unsafe { drop(CString::from_raw(value as *mut c_char)) };
}

#[test]
fn valid_owned_output_is_released_after_copying() {
    let _serial = serial();
    reset(1);
    let result = Library::start("context");
    assert_eq!(RELEASES.load(Ordering::SeqCst), 1, "owned output leaked");
    assert_eq!(
        result.as_deref(),
        Ok("copied envelope"),
        "copied output did not survive its release"
    );
}

#[test]
fn malformed_owned_output_is_released() {
    let _serial = serial();
    reset(2);
    let result = Library::dispatch("state", "event");
    assert_eq!(
        RELEASES.load(Ordering::SeqCst),
        1,
        "malformed owned output leaked"
    );
    assert!(result.is_err(), "malformed UTF-8 was accepted: {result:?}");
}

#[test]
fn null_output_is_not_released() {
    let _serial = serial();
    reset(0);
    let result = Library::start("context");
    assert!(result.is_err(), "null output was accepted: {result:?}");
    assert_eq!(
        RELEASES.load(Ordering::SeqCst),
        0,
        "null output was released"
    );
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

fn observation_output() -> *const c_char {
    let observed = snapshot_observation().unwrap_or_else(|error| error);
    owned(observed.as_bytes())
}

fn nested_output() -> *const c_char {
    MODE.store(0, Ordering::SeqCst);
    let problem = Library::start("nested")
        .err()
        .unwrap_or_else(|| "accepted nested call".to_owned());
    MODE.store(4, Ordering::SeqCst);
    owned(problem.as_bytes())
}

fn prepare_snapshot() -> std::sync::MutexGuard<'static, ()> {
    let guard = serial();
    reset(3);
    assert_eq!(put_slice("snapshot-test", "before"), Ok(()));
    guard
}

#[test]
fn start_pins_imports_across_a_concurrent_slice_replacement() {
    let _serial = prepare_snapshot();
    assert_eq!(
        Library::start("context").as_deref(),
        Ok("before|before|after")
    );
    assert_eq!(
        Library::start("context").as_deref(),
        Ok("after|after|after")
    );
}

#[test]
fn dispatch_pins_imports_across_a_concurrent_slice_replacement() {
    let _serial = prepare_snapshot();
    assert_eq!(
        Library::dispatch("state", "event").as_deref(),
        Ok("before|before|after")
    );
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

fn main_observation() -> Option<String> {
    MAIN_OUTPUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

#[test]
fn incompatible_import_ownership_versions_are_refused_before_exports() {
    let _serial = serial();
    for version in [0, 2] {
        reset(1);
        ABI.store(version, Ordering::SeqCst);
        assert!(Library::init().is_err(), "unsupported ABI initialized");
        assert!(
            Library::start("context").is_err(),
            "unsupported ABI started"
        );
        assert!(
            Library::dispatch("state", "event").is_err(),
            "unsupported ABI dispatched"
        );
        assert_eq!(RELEASES.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn malformed_output_does_not_leave_an_import_snapshot_active() {
    let _serial = prepare_snapshot();
    MODE.store(2, Ordering::SeqCst);
    assert!(Library::dispatch("state", "event").is_err());
    MODE.store(3, Ordering::SeqCst);
    assert_eq!(
        Library::start("context").as_deref(),
        Ok("before|before|after")
    );
}

#[test]
fn a_nested_export_is_refused_without_disturbing_the_outer_scope() {
    let _serial = prepare_snapshot();
    MODE.store(4, Ordering::SeqCst);
    assert_eq!(
        Library::start("context").as_deref(),
        Ok("A native call is already active")
    );
    MODE.store(3, Ordering::SeqCst);
    assert_eq!(
        Library::start("context").as_deref(),
        Ok("before|before|after")
    );
}

#[test]
fn nul_input_does_not_leave_an_import_snapshot_active() {
    let _serial = prepare_snapshot();
    assert!(Library::start("bad\0context").is_err());
    assert_eq!(
        Library::start("context").as_deref(),
        Ok("before|before|after")
    );
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

fn allocation_output() -> *const c_char {
    let observed = bounded_observation().unwrap_or_else(|error| error);
    owned(observed.as_bytes())
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
    assert_eq!(Library::start("context"), Ok(original));
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

fn empty_key_output() -> *const c_char {
    let invalid = [0xff_u8, 0];
    let missing = held("missing-import-key").expect("valid key");
    let pointers = [
        native_slice(std::ptr::null()),
        native_slice(invalid.as_ptr().cast()),
        native_slice(missing.as_ptr()),
    ];
    let texts: Result<Vec<_>, _> = pointers.into_iter().map(import_text).collect();
    owned(format!("{texts:?}").as_bytes())
}

#[test]
fn missing_null_and_invalid_keys_remain_empty_inside_a_call() {
    let _serial = serial();
    reset(6);
    assert_eq!(
        Library::start("context").as_deref(),
        Ok("Ok([\"\", \"\", \"\"])")
    );
}

#[test]
fn a_rust_unwind_releases_the_import_scope() {
    let _serial = prepare_snapshot();
    let result = std::panic::catch_unwind(|| {
        let _snapshot = ImportSnapshot::begin().expect("supported ABI and idle scope");
        panic!("unwind cleanup control");
    });
    assert!(result.is_err());
    MODE.store(1, Ordering::SeqCst);
    assert_eq!(Library::start("context").as_deref(), Ok("copied envelope"));
}

fn nested_dispatch_output() -> *const c_char {
    let observations = [("event", 1), ("bad\0event", 1), ("event", 2)].map(|(event, abi)| {
        native_shape(934, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0, 0);
        ABI.store(abi, Ordering::SeqCst);
        MODE.store(0, Ordering::SeqCst);
        let refused = Library::dispatch("nested", event).is_err();
        MODE.store(7, Ordering::SeqCst);
        ABI.store(1, Ordering::SeqCst);
        let shapes = take_stream()
            .expect("healthy outer scene stream")
            .into_iter()
            .filter(|shape| shape.code == 934)
            .count();
        format!("{refused}|{shapes}")
    });
    owned(observations.join(",").as_bytes())
}

#[test]
fn refused_nested_dispatch_preserves_the_outer_calls_streamed_shapes() {
    let _serial = serial();
    reset(7);
    assert_eq!(
        Library::start("context").as_deref(),
        Ok("true|1,true|1,true|1")
    );
}

#[test]
fn an_import_outside_a_call_is_reported_before_any_export_runs() {
    let _serial = serial();
    reset(1);
    let key = held("outside-call").expect("valid key");
    assert_eq!(import_text(native_slice(key.as_ptr())), Ok(String::new()));
    assert!(
        Library::start("context").is_err(),
        "callback fault became a successful envelope"
    );
    assert_eq!(
        RELEASES.load(Ordering::SeqCst),
        0,
        "export ran after callback fault"
    );
}

fn poisoned_callback_output() -> *const c_char {
    poison(slices());
    let key = held("snapshot-test").expect("valid key");
    let _ = native_slice(key.as_ptr());
    if MODE.load(Ordering::SeqCst) == 9 {
        let _ = Library::start("nested");
    }
    owned(b"fake successful envelope")
}

#[test]
fn a_poisoned_import_reports_failure_and_releases_the_export_result() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(8);
    MODE.store(1, Ordering::SeqCst);
    assert_eq!(Library::start("context").as_deref(), Ok("copied envelope"));
}

#[test]
fn a_refused_nested_call_cannot_consume_the_outer_import_fault() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(9);
}

fn assert_import_failure(mode: usize) -> std::sync::MutexGuard<'static, ()> {
    let guard = serial();
    reset(mode);
    let result = Library::start("context");
    slices().clear_poison();
    STREAM.clear_poison();
    assert!(
        result.is_err(),
        "import failure {mode} became a successful envelope"
    );
    assert_eq!(
        RELEASES.load(Ordering::SeqCst),
        1,
        "failed export output leaked"
    );
    guard
}

fn poisoned_stream_output() -> *const c_char {
    poison(&STREAM);
    if MODE.load(Ordering::SeqCst) == 10 {
        native_shape(934, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0, 0);
    } else if MODE.load(Ordering::SeqCst) == 11 {
        drop(take_stream());
    }
    owned(b"fake successful envelope")
}

#[test]
fn a_failed_shape_write_cannot_become_a_successful_envelope() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(10);
}

#[test]
fn a_failed_stream_read_cannot_become_a_successful_envelope() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(11);
}

#[test]
fn stream_poison_without_a_later_callback_still_rejects_the_export() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(12);
}
