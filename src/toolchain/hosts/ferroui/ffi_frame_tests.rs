//! The frame's own tests ([NATIVE-SHM], [NATIVE-SHM-SAFETY]): a successful export's frame is
//! copied out and consumed so the view survives the next frame's lease, a refused write or a
//! failed answer never becomes a successful view, and a frame the bridge sealed but no check
//! accepts is refused whole, with nothing leaking into the next frame.

use super::*;
use crate::frame;
use crate::rows::Kind;
use crate::scene::Shape;

/// A healthy `open` of the bench library.
fn opened() -> Result<crate::rows::Shown, String> {
    Library::open(0, 0, "test", "/home", false)
}

/// Whether `scene` letters exactly `text`.
fn letters(scene: &crate::rows::Scene, text: &str) -> bool {
    scene
        .shapes
        .iter()
        .any(|shape| matches!(shape, Shape::Lettering { text: shown, .. } if shown == text))
}

/// Whether the canvas scene of `view` letters `text`.
fn shows(view: &crate::rows::Shown, text: &str) -> bool {
    view.scenes
        .get(&0)
        .is_some_and(|scene| letters(scene, text))
}

/// The next export after a failed one succeeds and its frame holds none of the failed call's rows.
fn assert_clean_frame() {
    assert!(frame::take().is_none(), "a failed export left a frame");
    MODE.store(1, Ordering::SeqCst);
    let shown = opened().expect("the healthy call's frame");
    assert_eq!(shown.controls.len(), 1, "{shown:?}");
    let scene = shown.scenes.get(&0).expect("the healthy frame's scene");
    assert_eq!(scene.shapes.len(), 1, "{scene:?}");
    assert!(
        frame::take().is_none(),
        "the healthy frame was left pending"
    );
}

/// Mode `mode` is refused whole and the next export draws a clean frame.
fn assert_refused_then_clean(mode: usize) {
    let _serial = assert_import_failure(mode);
    assert_clean_frame();
}

#[test]
fn a_checked_frame_survives_its_lease_and_is_read_once() {
    let _serial = serial();
    reset(1);
    let shown = opened().expect("a checked frame");
    assert!(
        frame::take().is_none(),
        "the checked frame was left pending"
    );
    assert_eq!(shown.controls.len(), 1);
    assert_eq!(shown.controls[0].kind, Kind::Canvas);
    assert!(
        shows(&shown, "Count 0"),
        "the scene text did not survive its lease: {shown:?}"
    );
}

#[test]
fn a_malformed_frame_is_refused_and_consumed() {
    let _serial = assert_import_failure(2);
    MODE.store(1, Ordering::SeqCst);
    assert_eq!(opened().map(|shown| shown.controls.len()), Ok(1));
}

#[test]
fn a_call_that_streams_nothing_reads_an_empty_view() {
    let _serial = serial();
    reset(0);
    let shown = opened().expect("an empty view");
    assert!(shown.controls.is_empty(), "an empty frame drew controls");
    assert!(shown.roots.is_empty(), "an empty frame drew roots");
    assert!(shown.asks.is_empty(), "an empty frame asked for work");
    assert!(frame::take().is_none(), "the empty frame was left pending");
}

#[test]
fn valid_owned_output_is_released_after_copying() {
    let _serial = serial();
    reset(1);
    let shown = opened().expect("a checked frame");
    assert!(
        frame::take().is_none(),
        "the frame was left leased after it was copied"
    );
    MODE.store(13, Ordering::SeqCst);
    let next = opened().expect("the next call's frame");
    assert!(
        shows(&next, "Count 1"),
        "the next frame did not carry its own count: {next:?}"
    );
    assert!(
        shows(&shown, "Count 0"),
        "the copied view did not survive the next frame's lease: {shown:?}"
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
    let shown = opened().expect("a checked frame");
    MODE.store(7, Ordering::SeqCst);
    let next = Library::event(0, &Touch::control(0, "")).expect("the event's frame");
    let scene = next.scenes.get(&0).expect("the event frame's scene");
    assert_eq!(scene.shapes.len(), 3, "the event frame's own shapes");
    assert!(
        shows(&shown, "Count 0"),
        "the view copied at open did not survive the event's frame: {shown:?}"
    );
}

/// Control text, then scene text, that is not UTF-8.
#[test]
fn malformed_owned_output_is_released() {
    assert_refused_then_clean(14);
    assert_refused_then_clean(15);
}

#[test]
fn null_output_is_not_released() {
    let _serial = assert_import_failure(16);
    assert_eq!(EXPORTS.load(Ordering::SeqCst), 1, "the export ran");
    MODE.store(1, Ordering::SeqCst);
    let shown = opened().expect("the next call's frame");
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

/// A failed shape write, a failed view-row write, a scene count past its rows, an unknown
/// capability code, and shapes streamed by a failed export: none becomes a view or leaks forward.
#[test]
fn a_failed_write_or_read_cannot_become_a_successful_envelope() {
    for mode in [10, 18, 19, 20, 12] {
        assert_refused_then_clean(mode);
    }
}
