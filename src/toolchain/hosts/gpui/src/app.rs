//! The application's entry points and the shared-memory bridge, linked from its static archive
//! ([NATIVE-SHM-OWNERSHIP]). FerroUI on Windows loads the application's DLL instead
//! ([WINUI-HOST-DLL]) and provides the same names from its own loader.

use crate::frame::{Instance, Lease};
use std::ffi::{c_char, c_void};

/// An opaque bridge, owned by the application's runtime.
#[repr(C)]
pub(crate) struct Bridge {
    _opaque: [u8; 0],
}

extern "C" {
    /// Runs the application's top-level statements and `fn main`; call first.
    pub(crate) fn osprey_main() -> i32;
    /// The application's `argument` export: one launch argument, in order, before `open`.
    pub(crate) fn osprey_app_argument(text: *const c_char) -> i64;
    /// The application's `open` export: draws the first frame.
    pub(crate) fn osprey_app_open(
        now: i64,
        offset: i64,
        platform: *const c_char,
        home: *const c_char,
        dark: bool,
    ) -> i64;
    /// The application's `event` export: delivers one host event and draws the next frame.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn osprey_app_event(
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
    pub(crate) fn osprey_host_abi_version() -> i32;
    pub(crate) fn osp_native_open(
        instance: *const Instance,
        fingerprint: *const c_char,
        bytes: i64,
        chunks: i64,
        owners: i64,
        out: *mut *mut Bridge,
    ) -> i32;
    pub(crate) fn osp_native_frame_commit(bridge: *mut Bridge, revision: *mut i64) -> i32;
    pub(crate) fn osp_native_acquire(
        bridge: *mut Bridge,
        out: *mut Lease,
        revision: *mut i64,
    ) -> i32;
    pub(crate) fn osp_native_child(
        bridge: *mut Bridge,
        lease: *const Lease,
        index: i64,
        out: *mut Lease,
    ) -> i32;
    pub(crate) fn osp_native_length(
        bridge: *mut Bridge,
        lease: *const Lease,
        length: *mut i64,
    ) -> i32;
    pub(crate) fn osp_native_view(
        bridge: *mut Bridge,
        lease: *const Lease,
        kind: i64,
        offset: i64,
        length: i64,
        out: *mut *const c_void,
    ) -> i32;
    pub(crate) fn osp_native_release(bridge: *mut Bridge, lease: *const Lease) -> i32;
    pub(crate) fn osp_native_frame_discard();
}

/// A linked archive is always present.
pub(crate) fn load() -> Result<(), String> {
    Ok(())
}
