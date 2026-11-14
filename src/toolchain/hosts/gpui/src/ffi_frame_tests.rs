//! The frame's own tests ([NATIVE-SHM], [NATIVE-SHM-SAFETY]): a successful export's frame is
//! copied out and consumed so the view survives the next frame's lease, a refused write or a
//! failed answer never becomes a successful view, and a frame the bridge sealed but no check
//! accepts is refused whole, with nothing leaking into the next frame.

use super::*;
use crate::frame;
use crate::rows::Kind;
use crate::scene::Shape;

/// The next export after a failed one succeeds and its frame holds none of the failed call's rows.
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

#[test]
fn valid_owned_output_is_released_after_copying() {
    let _serial = serial();
    reset(1);
    let shown = Library::open(0, 0, "test", "/home", false).expect("a checked frame");
    assert!(
        frame::take().is_none(),
        "the frame was left leased after it was copied"
    );
    MODE.store(13, Ordering::SeqCst);
    let next = Library::open(0, 0, "test", "/home", false).expect("the next call's frame");
    let counted = next.scenes.get(&0).expect("the next frame's scene");
    assert!(
        counted
            .shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Lettering { text, .. } if text == "Count 1")),
        "the next frame did not carry its own count: {counted:?}"
    );
    let held = shown.scenes.get(&0).expect("the first frame's scene");
    assert!(
        held.shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Lettering { text, .. } if text == "Count 0")),
        "the copied view did not survive the next frame's lease: {held:?}"
    );
    assert!(
        frame::take().is_none(),
        "the next frame was left leased after it was copied"
    );
}

#[test]
fn valid_owned_output_is_released_after_copying_extra_held_across_an_event() {
    let _serial = serial();
    reset(1);
    let shown = Library::open(0, 0, "test", "/home", false).expect("a checked frame");
    MODE.store(7, Ordering::SeqCst);
    let next = Library::event(0, &Touch::control(0, "")).expect("the event's frame");
    let scene = next.scenes.get(&0).expect("the event frame's scene");
    assert_eq!(scene.shapes.len(), 3, "the event frame's own shapes");
    let held = shown.scenes.get(&0).expect("the first frame's scene");
    assert!(
        held.shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Lettering { text, .. } if text == "Count 0")),
        "the view copied at open did not survive the event's frame: {held:?}"
    );
}

#[test]
fn malformed_owned_output_is_released() {
    let _serial = serial();
    reset(14);
    let result = Library::open(0, 0, "test", "/home", false);
    assert!(
        result.is_err(),
        "control text that is not UTF-8 was accepted: {result:?}"
    );
    assert!(
        frame::take().is_none(),
        "the malformed frame was left pending"
    );
    assert_clean_frame();
}

#[test]
fn malformed_owned_output_is_released_extra_scene_text() {
    let _serial = serial();
    reset(15);
    let result = Library::open(0, 0, "test", "/home", false);
    assert!(
        result.is_err(),
        "scene text that is not UTF-8 was accepted: {result:?}"
    );
    assert!(
        frame::take().is_none(),
        "the malformed frame was left pending"
    );
    assert_clean_frame();
}

#[test]
fn null_output_is_not_released() {
    let _serial = serial();
    reset(16);
    let result = Library::open(0, 0, "test", "/home", false);
    assert!(
        result.is_err(),
        "a failed export answer became a successful view: {result:?}"
    );
    assert!(
        frame::take().is_none(),
        "the failed export's frame was left pending"
    );
    assert_eq!(EXPORTS.load(Ordering::SeqCst), 1, "the export ran");
    MODE.store(1, Ordering::SeqCst);
    let shown = Library::open(0, 0, "test", "/home", false).expect("the next call's frame");
    assert_eq!(shown.controls.len(), 1);
}

#[test]
fn null_output_is_not_released_extra_a_refused_event_still_draws() {
    let _serial = serial();
    reset(17);
    let shown = Library::event(0, &Touch::control(0, "")).expect("a refused event still draws");
    assert_eq!(shown.controls.len(), 1, "{shown:?}");
    assert_eq!(
        shown.diagnostics().collect::<Vec<_>>(),
        ["the picker was closed"],
        "the refused event's reason did not ride along"
    );
    assert!(
        frame::take().is_none(),
        "the refused event's frame was left pending"
    );
}

#[test]
fn a_failed_shape_write_cannot_become_a_successful_envelope() {
    let _serial = assert_import_failure(10);
    assert_clean_frame();
}

#[test]
fn a_failed_shape_write_cannot_become_a_successful_envelope_extra_view_rows() {
    let _serial = assert_import_failure(18);
    assert_clean_frame();
}

#[test]
fn a_failed_stream_read_cannot_become_a_successful_envelope() {
    let _serial = serial();
    reset(19);
    let result = Library::open(0, 0, "test", "/home", false);
    assert!(
        result.is_err(),
        "a scene count past its rows became a successful view: {result:?}"
    );
    assert!(
        frame::take().is_none(),
        "the unreadable frame was left pending"
    );
    assert_clean_frame();
}

#[test]
fn a_failed_stream_read_cannot_become_a_successful_envelope_extra_ask_code() {
    let _serial = serial();
    reset(20);
    let result = Library::open(0, 0, "test", "/home", false);
    assert!(
        result.is_err(),
        "an unknown capability code became a successful view: {result:?}"
    );
    assert!(
        frame::take().is_none(),
        "the unreadable frame was left pending"
    );
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
