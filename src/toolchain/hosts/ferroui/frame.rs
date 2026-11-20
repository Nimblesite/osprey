//! A call's frame, read in place from the application's shared bridge ([NATIVE-SHM-SCENE],
//! [NATIVE-SHM-VIEW], [NATIVE-SHM-ASK], [NATIVE-SHM-OWNERSHIP], [NATIVE-SHM-PUBLISH]). The Osprey
//! runtime writes every control, scene row and ask once into bridge storage; after an export
//! returns the host seals those runs, leases them and reads the same bytes. Nothing is encoded,
//! queued, locked or copied per row, and a frame the painter still holds outlives the frames
//! that replace it. `rows.rs` checks every row before anything is drawn.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

/// One scene row: the generated `SceneShape` layout ([NATIVE-SHM-SCENE]). `native.td` lists the
/// codes; text names bytes of the frame's text run.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Streamed {
    pub code: i64,
    pub index: i64,
    pub values: [f64; 5],
    pub colour: i64,
    pub depth: i64,
    pub text_offset: i64,
    pub text_length: i64,
}

/// One control: the generated `ViewRow` layout ([NATIVE-SHM-VIEW]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewRow {
    pub kind: i64,
    pub parent: i64,
    pub action: i64,
    pub text_offset: i64,
    pub text_length: i64,
    pub colour: i64,
    pub background: i64,
    pub size: f64,
    pub padding: f64,
    pub spacing: f64,
    pub radius: f64,
    pub weight: i64,
    pub width: f64,
    pub height: f64,
    pub grow: f64,
    pub flags: i64,
    pub border: i64,
    pub hint_offset: i64,
    pub hint_length: i64,
}

/// One field of an ask: the generated `AskRow` layout ([NATIVE-SHM-ASK]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AskRow {
    pub field: i64,
    pub number: i64,
    pub text_offset: i64,
    pub text_length: i64,
}

/// The generated `OwnedLease` descriptor: opaque to the host, 64 bytes, 8-aligned.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct Lease([u8; 64]);

/// The generated `ApplicationInstance`: a 16-byte identity, fresh for each process.
#[repr(C, align(8))]
struct Instance([u8; 16]);

const _: () = assert!(size_of::<Streamed>() == 88 && align_of::<Streamed>() == 8);
const _: () = assert!(size_of::<ViewRow>() == 152 && align_of::<ViewRow>() == 8);
const _: () = assert!(size_of::<AskRow>() == 32 && align_of::<AskRow>() == 8);

/// The frame's runs, in child order; each run's chunk type is its index plus one.
const RUNS: usize = 4;
const SHAPES: usize = 0;
const VIEWS: usize = 1;
const TEXT: usize = 2;
const ASKS: usize = 3;
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
    "the application sent more rows than one frame may hold",
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
    fn osp_native_frame_discard();
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
    format!("The shared bridge refused the frame: {reason}")
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

/// One sealed run of a frame and the bytes its lease keeps alive.
struct Leased {
    lease: Lease,
    bytes: *const u8,
    length: usize,
}

/// One call's sealed runs, leased from the bridge until dropped.
pub struct Frame {
    runs: Vec<Leased>,
}

// SAFETY: a sealed chunk is immutable and its lease is released through the bridge's mutex.
unsafe impl Send for Frame {}
// SAFETY: as above; shared readers only ever read the sealed rows.
unsafe impl Sync for Frame {}

impl std::fmt::Debug for Frame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Frame({} controls, {} scene rows, {} asks)",
            self.views().len(),
            self.shapes().len(),
            self.asks().len()
        )
    }
}

/// Leases child `index` of `root` and borrows its bytes for as long as the lease lives.
fn leased_run(bridge: *mut Bridge, root: &Lease, index: usize) -> Result<Leased, String> {
    let mut run = Leased {
        lease: Lease([0; 64]),
        bytes: std::ptr::null(),
        length: 0,
    };
    let (mut length, mut view) = (0, std::ptr::null());
    let child = i64::try_from(index).map_err(|_| refusal(7))?;
    // SAFETY: `root` is a live owner; on success `run.lease` owns the child.
    checked(unsafe { osp_native_child(bridge, root, child, &mut run.lease) })?;
    // SAFETY: `run.lease` is the live owner acquired above.
    let measured = checked(unsafe { osp_native_length(bridge, &run.lease, &mut length) });
    // SAFETY: the view borrows from `run.lease`, which the frame keeps until it drops.
    let viewed = measured.and_then(|()| {
        checked(unsafe { osp_native_view(bridge, &run.lease, child + 1, 0, length, &mut view) })
    });
    if let Err(problem) = viewed {
        // SAFETY: the child lease is released exactly once on this failure path.
        let _ = unsafe { osp_native_release(bridge, &run.lease) };
        return Err(problem);
    }
    run.bytes = view.cast();
    run.length = usize::try_from(length).map_err(|_| refusal(7))?;
    Ok(run)
}

impl Frame {
    /// Leases the published root's four runs and releases the root itself.
    fn leased(bridge: *mut Bridge) -> Result<Self, String> {
        let mut revision = 0;
        let mut root = Lease([0; 64]);
        // SAFETY: the out-pointers are live locals; on success `root` owns the published frame.
        checked(unsafe { osp_native_acquire(bridge, &mut root, &mut revision) })?;
        let mut frame = Self { runs: Vec::new() };
        let mut problem = Ok(());
        for index in 0..RUNS {
            match leased_run(bridge, &root, index) {
                Ok(run) => frame.runs.push(run),
                Err(refused) => {
                    problem = Err(refused);
                    break;
                }
            }
        }
        // SAFETY: `root` is released exactly once; each run's own lease keeps its bytes alive.
        checked(unsafe { osp_native_release(bridge, &root) })?;
        problem.map(|()| frame)
    }

    /// The rows of run `index` as `T`: its length is a whole number of rows, or the run is refused.
    fn rows<T>(&self, index: usize) -> &[T] {
        match self.runs.get(index) {
            Some(run)
                if !run.bytes.is_null() && run.length % size_of::<T>() == 0 && run.length > 0 =>
            {
                // SAFETY: the bridge checked `length` bytes of sealed, 8-aligned storage under this
                // lease, the length is a whole number of rows, and every bit pattern is a valid
                // row of plain integers and floats.
                unsafe { std::slice::from_raw_parts(run.bytes.cast(), run.length / size_of::<T>()) }
            }
            _ => &[],
        }
    }

    /// The call's scene rows in the order the application drew them.
    pub fn shapes(&self) -> &[Streamed] {
        self.rows(SHAPES)
    }

    /// The call's controls in drawing order.
    pub fn views(&self) -> &[ViewRow] {
        self.rows(VIEWS)
    }

    /// The UTF-8 bytes every row's text names.
    pub fn text(&self) -> &[u8] {
        self.rows(TEXT)
    }

    /// The call's requests and slice writes, as ask fields in order.
    pub fn asks(&self) -> &[AskRow] {
        self.rows(ASKS)
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        if let Ok(bridge) = bridge() {
            for run in &self.runs {
                // SAFETY: each lease is this frame's own, released exactly once. A refusal here
                // means the bridge already faulted and keeps the storage pinned, which is safe.
                let _ = unsafe { osp_native_release(bridge, &run.lease) };
            }
        }
    }
}

/// The frame sealed by the last export, until the host reads it.
static PENDING: Mutex<Option<Arc<Frame>>> = Mutex::new(None);

fn pending() -> std::sync::MutexGuard<'static, Option<Arc<Frame>>> {
    // The slot holds one owned value, so a panic elsewhere cannot leave it half-written.
    PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Seals the runs the export that just returned wrote and holds them for `take`. A run the
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

/// Drops whatever a call wrote without publishing it: for a call, such as `osprey_main`, whose
/// rows belong to no frame.
pub fn discard() {
    *pending() = None;
    // SAFETY: called on the application's executor thread, between exports.
    unsafe { osp_native_frame_discard() };
}

/// The last export's frame, handed over once; `None` when no export sealed one.
pub fn take() -> Option<Arc<Frame>> {
    pending().take()
}
