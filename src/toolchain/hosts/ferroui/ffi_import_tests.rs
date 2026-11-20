//! The pinning tests of the host-held slices an export reads through `native_slice`
//! ([GPUI-HOST-BOUNDARY], [NATIVE-SHM-EVENT]): an export's imports stay pinned across concurrent
//! slice replacements, a faulting or nested call releases its scope, and nothing a failed call saw
//! leaks into the next one.

use super::test_support::isolated_test;
use super::*;
use crate::frame;

/// One snapshot-pinning export, run against a slice a background thread replaces mid-call.
fn prepare_snapshot() -> std::sync::MutexGuard<'static, ()> {
    let guard = serial();
    reset(3);
    assert_eq!(put_slice("snapshot-test", "before"), Ok(()));
    guard
}

#[test]
fn start_pins_imports_across_a_concurrent_slice_replacement() {
    let _serial = prepare_snapshot();
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some("before|before|after"));
    assert!(Library::open(0, 0, "test", "/home", false).is_ok());
    assert_eq!(snapshot().as_deref(), Some("after|after|after"));
}

#[test]
fn start_pins_imports_across_a_concurrent_slice_replacement_extra_argument_call() {
    let _serial = prepare_snapshot();
    assert_eq!(Library::arguments(&["given".to_owned()]), Ok(()));
    assert_eq!(snapshot().as_deref(), Some("before|before|after"));
    assert_eq!(EXPORTS.load(Ordering::SeqCst), 1, "one export per argument");
}

#[test]
fn dispatch_pins_imports_across_a_concurrent_slice_replacement() {
    let _serial = prepare_snapshot();
    assert!(Library::event(0, &Touch::control(0, "")).is_ok());
    assert_eq!(snapshot().as_deref(), Some("before|before|after"));
}

#[test]
fn dispatch_pins_imports_across_a_concurrent_slice_replacement_extra_key_event() {
    let _serial = prepare_snapshot();
    assert!(Library::event(0, &Touch::key(0, "left", true)).is_ok());
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

#[test]
fn a_poisoned_import_reports_failure_and_releases_the_export_result() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(8);
    MODE.store(1, Ordering::SeqCst);
    let shown = Library::open(0, 0, "test", "/home", false).expect("the next call's frame");
    assert_eq!(shown.controls.len(), 1);
}

#[test]
fn a_poisoned_import_reports_failure_and_releases_the_export_result_extra_event_call() {
    if isolated_test() {
        return;
    }
    let _serial = serial();
    reset(8);
    let result = Library::event(0, &Touch::control(0, ""));
    slices().clear_poison();
    assert!(
        result.is_err(),
        "a poisoned import during an event became a successful view"
    );
    assert!(
        frame::take().is_none(),
        "a poisoned import during an event left a frame"
    );
    MODE.store(1, Ordering::SeqCst);
    let shown = Library::event(0, &Touch::control(0, "")).expect("the next event's frame");
    assert_eq!(shown.controls.len(), 1);
}

#[test]
fn a_refused_nested_call_cannot_consume_the_outer_import_fault() {
    if isolated_test() {
        return;
    }
    let _serial = assert_import_failure(9);
}

#[test]
fn stream_poison_without_a_later_callback_still_rejects_the_export() {
    if isolated_test() {
        return;
    }
    let _serial = serial();
    reset(1);
    poison(slices());
    let result = Library::open(0, 0, "test", "/home", false);
    slices().clear_poison();
    assert!(
        result.is_err(),
        "a poisoned store with no import during the call became a successful view"
    );
    assert!(frame::take().is_none(), "the rejected call left a frame");
    assert_eq!(EXPORTS.load(Ordering::SeqCst), 0, "an export ran");
    let shown = Library::open(0, 0, "test", "/home", false).expect("the next call's frame");
    assert_eq!(shown.controls.len(), 1);
}

#[test]
fn stream_poison_without_a_later_callback_still_rejects_the_export_extra_event() {
    if isolated_test() {
        return;
    }
    let _serial = serial();
    reset(1);
    poison(slices());
    let result = Library::event(0, &Touch::pointer(0, 1., 2., true, false));
    slices().clear_poison();
    assert!(
        result.is_err(),
        "a poisoned store with no import during the event became a successful view"
    );
    assert!(frame::take().is_none(), "the rejected event left a frame");
    assert_eq!(EXPORTS.load(Ordering::SeqCst), 0, "an export ran");
}
