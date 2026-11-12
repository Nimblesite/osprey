//! A call's streamed shapes, read in place from the application's shared bridge
//! ([NATIVE-SHM-SCENE], [NATIVE-SHM-OWNERSHIP], [NATIVE-SHM-PUBLISH]). The Osprey runtime writes
//! each rect, disc and sprite once into bridge storage; after an export returns the host seals
//! that run, leases it and draws from the same bytes. Nothing is queued, locked or copied per
//! shape, and a frame the painter still holds outlives the frames that replace it.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::ops::Deref;
use std::sync::{Arc, Mutex, OnceLock};

/// One rect, disc or sprite: the generated `SceneShape` layout ([UI-SCENE-STREAM]). The fields are
/// the scene encoding's code, a sprite's atlas index, five floats, the packed colour and the depth.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Streamed {
    pub code: i64,
    pub index: i64,
    pub values: [f64; 5],
    pub colour: i64,
    pub depth: i64,
}

/// The generated `OwnedLease` descriptor: opaque to the host, 64 bytes, 8-aligned.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct Lease([u8; 64]);

/// The generated `ApplicationInstance`: a 16-byte identity, fresh for each process.
#[repr(C, align(8))]
struct Instance([u8; 16]);

const _: () = assert!(size_of::<Streamed>() == 72 && align_of::<Streamed>() == 8);

/// The chunk type of a sealed shape run (`OSP_NATIVE_SCENE_SHAPES`).
const SHAPES: i64 = 1;
/// Everything the bridge may hold at once, counting frames the painter still leases.
const BYTES: i64 = 512 << 20;
const CHUNKS: i64 = 1024;
const OWNERS: i64 = 4096;
/// The `BridgeError` variants, in the generated discriminant order.
const REFUSALS: [&str; 13] = [
    "an invalid limit",
    "its memory budget is spent",
    "no free slot",
    "a lease from another application instance",
    "a stale allocation",
    "an invalid owner",
    "an invalid range",
    "a layout this host was not built for",
    "an exhausted counter",
    "the bridge is closing",
    "the bridge is closed",
    "the application sent more shapes than one frame may hold",
    "a failed lock",
];

#[repr(C)]
struct Bridge {
    _opaque: [u8; 0],
}

extern "C" {
    fn osp_native_open(
        instance: *const Instance,
        fingerprint: *const std::ffi::c_char,
        bytes: i64,
        chunks: i64,
        owners: i64,
        out: *mut *mut Bridge,
    ) -> i32;
    fn osp_native_frame_commit(bridge: *mut Bridge, revision: *mut i64) -> i32;
    fn osp_native_acquire(bridge: *mut Bridge, out: *mut Lease, revision: *mut i64) -> i32;
    fn osp_native_child(
        bridge: *mut Bridge,
        lease: *const Lease,
        index: i64,
        out: *mut Lease,
    ) -> i32;
    fn osp_native_length(bridge: *mut Bridge, lease: *const Lease, length: *mut i64) -> i32;
    fn osp_native_view(
        bridge: *mut Bridge,
        lease: *const Lease,
        kind: i64,
        offset: i64,
        length: i64,
        out: *mut *const std::ffi::c_void,
    ) -> i32;
    fn osp_native_release(bridge: *mut Bridge, lease: *const Lease) -> i32;
}

/// The process's one bridge. It is never destroyed: leases may outlive every window.
struct Opened(*mut Bridge);

// SAFETY: every bridge entry point takes the instance's own mutex.
unsafe impl Send for Opened {}
// SAFETY: as above; the pointer itself is never written after `open`.
unsafe impl Sync for Opened {}

fn refusal(status: i32) -> String {
    let reason = usize::try_from(status - 1)
        .ok()
        .and_then(|tag| REFUSALS.get(tag))
        .copied()
        .unwrap_or("an unknown failure");
    format!("The shared scene bridge refused the frame: {reason}")
}

fn checked(status: i32) -> Result<(), String> {
    match status {
        0 => Ok(()),
        other => Err(refusal(other)),
    }
}

fn fresh_instance() -> Instance {
    let half = || u128::from(RandomState::new().build_hasher().finish());
    Instance(((half() << 64) | half()).to_le_bytes())
}

fn open() -> Result<Opened, String> {
    // The layout this host was compiled against; a runtime built for another refuses to open.
    let fingerprint = std::ffi::CString::new(env!("OSPREY_NATIVE_FINGERPRINT"))
        .map_err(|_| "The shared layout fingerprint held a NUL byte".to_owned())?;
    let mut bridge = std::ptr::null_mut();
    // SAFETY: every pointer outlives the call and `bridge` is written only on success.
    let status = unsafe {
        osp_native_open(
            &fresh_instance(),
            fingerprint.as_ptr(),
            BYTES,
            CHUNKS,
            OWNERS,
            &mut bridge,
        )
    };
    checked(status).map(|()| Opened(bridge))
}

fn bridge() -> Result<*mut Bridge, String> {
    static BRIDGE: OnceLock<Result<Opened, String>> = OnceLock::new();
    BRIDGE
        .get_or_init(open)
        .as_ref()
        .map(|opened| opened.0)
        .map_err(Clone::clone)
}

/// One call's sealed shapes, leased from the bridge until dropped.
pub struct Frame {
    lease: Lease,
    rows: *const Streamed,
    count: usize,
    /// Whether `lease` is a live owner this frame must release.
    owned: bool,
}

// SAFETY: a sealed chunk is immutable and its lease is released through the bridge's mutex.
unsafe impl Send for Frame {}
// SAFETY: as above; shared readers only ever read the sealed rows.
unsafe impl Sync for Frame {}

impl std::fmt::Debug for Frame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Frame({} shapes)", self.count)
    }
}

impl Frame {
    /// Leases the published root and borrows its rows for as long as the lease lives.
    fn leased(bridge: *mut Bridge) -> Result<Self, String> {
        let mut frame = Self {
            lease: Lease([0; 64]),
            rows: std::ptr::null(),
            count: 0,
            owned: false,
        };
        let (mut revision, mut length, mut view) = (0, 0, std::ptr::null());
        let mut root = Lease([0; 64]);
        // SAFETY: the out-pointers are live locals; on success `root` owns the published frame.
        checked(unsafe { osp_native_acquire(bridge, &mut root, &mut revision) })?;
        // SAFETY: `root` is the live owner acquired above; the shape run is its first child.
        let shapes = unsafe { osp_native_child(bridge, &root, 0, &mut frame.lease) };
        frame.owned = shapes == 0;
        // SAFETY: `root` is released exactly once; the run's own lease keeps its rows alive.
        checked(unsafe { osp_native_release(bridge, &root) })?;
        checked(shapes)?;
        // SAFETY: `frame.lease` is the live owner acquired above.
        checked(unsafe { osp_native_length(bridge, &frame.lease, &mut length) })?;
        // SAFETY: the view borrows from `frame.lease`, which `frame` keeps until it drops.
        checked(unsafe { osp_native_view(bridge, &frame.lease, SHAPES, 0, length, &mut view) })?;
        frame.rows = view.cast();
        frame.count = usize::try_from(length).map_err(|_| refusal(7))? / size_of::<Streamed>();
        Ok(frame)
    }

    /// The call's shapes in the order the application drew them.
    pub fn shapes(&self) -> &[Streamed] {
        if self.rows.is_null() || self.count == 0 {
            return &[];
        }
        // SAFETY: the bridge checked `count` rows of sealed, 8-aligned storage under this lease,
        // and every bit pattern is a valid `Streamed`.
        unsafe { std::slice::from_raw_parts(self.rows, self.count) }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        if let (true, Ok(bridge)) = (self.owned, bridge()) {
            // SAFETY: the lease is this frame's own, released exactly once. A refusal here means
            // the bridge already faulted and keeps the storage pinned, which is safe.
            let _ = unsafe { osp_native_release(bridge, &self.lease) };
        }
    }
}

/// The frame sealed by the last export, until the envelope it belongs to is applied.
static PENDING: Mutex<Option<Arc<Frame>>> = Mutex::new(None);

fn pending() -> std::sync::MutexGuard<'static, Option<Arc<Frame>>> {
    // The slot holds one owned value, so a panic elsewhere cannot leave it half-written.
    PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Seals the shapes the export that just returned streamed and holds them for `take`. A run the
/// bridge refuses fails the export whole; the run is consumed either way, so none leaks forward.
pub fn seal() -> Result<(), String> {
    *pending() = None;
    let bridge = bridge()?;
    let mut revision = 0;
    // SAFETY: called on the application's executor thread, after its export returned.
    checked(unsafe { osp_native_frame_commit(bridge, &mut revision) })?;
    *pending() = Some(Arc::new(Frame::leased(bridge)?));
    Ok(())
}

/// The last export's frame, handed over once; `None` when no export sealed one.
pub fn take() -> Option<Arc<Frame>> {
    pending().take()
}

/// One scene's run of a frame's shapes; it keeps the frame leased ([UI-SCENE-STREAM]).
#[derive(Debug, Clone, Default)]
pub struct Run {
    frame: Option<Arc<Frame>>,
    start: usize,
    count: usize,
}

impl Deref for Run {
    type Target = [Streamed];

    fn deref(&self) -> &[Streamed] {
        self.frame
            .as_ref()
            .and_then(|frame| frame.shapes().get(self.start..self.start + self.count))
            .unwrap_or_default()
    }
}

/// Deals a frame's shapes out to the scenes that counted them, in drawing order.
#[derive(Debug, Default)]
pub struct Cursor {
    frame: Option<Arc<Frame>>,
    next: usize,
}

impl Cursor {
    /// A cursor at the first shape of `frame`; no frame deals nothing.
    pub fn new(frame: Option<Arc<Frame>>) -> Self {
        Self { frame, next: 0 }
    }

    /// The shapes no scene has taken yet.
    pub fn remaining(&self) -> usize {
        let total = self.frame.as_ref().map_or(0, |frame| frame.shapes().len());
        total.saturating_sub(self.next)
    }

    /// The next `count` shapes as one run, or how many were left instead.
    pub fn run(&mut self, count: usize) -> Result<Run, usize> {
        let left = self.remaining();
        if count > left {
            self.next += left;
            return Err(left);
        }
        let run = Run {
            frame: self.frame.clone(),
            start: self.next,
            count,
        };
        self.next += count;
        Ok(run)
    }
}

/// Streams one shape as the Osprey runtime does, for tests that stand in for an application.
#[cfg(test)]
pub(crate) fn stream(shape: Streamed) -> bool {
    extern "C" {
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
        ) -> i64;
    }
    let [x, y, width, height, angle] = shape.values;
    // SAFETY: the runtime's scalar-only entry point; tests call it on their serialized thread.
    let refused = unsafe {
        osp_native_scene_shape(
            shape.code,
            shape.index,
            x,
            y,
            width,
            height,
            angle,
            shape.colour,
            shape.depth,
        )
    };
    refused == 0
}
