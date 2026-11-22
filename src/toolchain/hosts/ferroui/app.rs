//! The application's entry points and the shared-memory bridge, as this platform provides them.
//!
//! On macOS and Linux the application is a static archive linked into the host, so these are
//! plain `extern "C"` declarations. On Windows it is the self-contained DLL every Windows host
//! loads ([WINUI-HOST-DLL]): the archive's MinGW runtime stays inside it, the host fills each
//! import slot through `osprey_import_<name>` before `osprey_main` ([WINUI-HOST-IMPORTS]), and the
//! bridge is reached through the exported `osprey_native_bridge` table ([NATIVE-SHM-OWNERSHIP]).

use std::ffi::{c_char, c_void};

/// An opaque bridge, owned by the application's runtime.
#[repr(C)]
pub(crate) struct Bridge {
    _opaque: [u8; 0],
}

#[cfg(not(windows))]
pub(crate) use linked::*;

#[cfg(not(windows))]
mod linked {
    use super::{c_char, c_void, Bridge};
    use crate::frame::{Instance, Lease};

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
}

#[cfg(windows)]
pub(crate) use loaded::*;

#[cfg(windows)]
mod loaded {
    use super::{c_char, c_void, Bridge};
    use crate::frame::{Instance, Lease};
    use std::path::PathBuf;
    use std::sync::OnceLock;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    type Main = unsafe extern "C" fn() -> i32;
    type Argument = unsafe extern "C" fn(*const c_char) -> i64;
    type Open = unsafe extern "C" fn(i64, i64, *const c_char, *const c_char, bool) -> i64;
    type Event = unsafe extern "C" fn(
        i64,
        i64,
        i64,
        *const c_char,
        *const c_char,
        f64,
        f64,
        bool,
        bool,
    ) -> i64;
    type Version = unsafe extern "C" fn() -> i32;
    type Binder = unsafe extern "C" fn(*const c_void);

    /// The bridge entries in the order of the DLL's exported table (`BRIDGE_ENTRIES`).
    #[repr(C)]
    struct BridgeTable {
        open: unsafe extern "C" fn(
            *const Instance,
            *const c_char,
            i64,
            i64,
            i64,
            *mut *mut Bridge,
        ) -> i32,
        frame_commit: unsafe extern "C" fn(*mut Bridge, *mut i64) -> i32,
        frame_discard: unsafe extern "C" fn(),
        acquire: unsafe extern "C" fn(*mut Bridge, *mut Lease, *mut i64) -> i32,
        child: unsafe extern "C" fn(*mut Bridge, *const Lease, i64, *mut Lease) -> i32,
        length: unsafe extern "C" fn(*mut Bridge, *const Lease, *mut i64) -> i32,
        view: unsafe extern "C" fn(
            *mut Bridge,
            *const Lease,
            i64,
            i64,
            i64,
            *mut *const c_void,
        ) -> i32,
        _retain: *const c_void,
        release: unsafe extern "C" fn(*mut Bridge, *const Lease) -> i32,
        _close: *const c_void,
        _destroy: *const c_void,
        _usage: *const c_void,
    }

    struct App {
        main: Main,
        argument: Argument,
        open: Open,
        event: Event,
        version: Version,
        bridge: &'static BridgeTable,
    }

    // SAFETY: function pointers and a table in a DLL that is never unloaded.
    unsafe impl Send for App {}
    // SAFETY: as above; nothing here is written after loading.
    unsafe impl Sync for App {}

    static APP: OnceLock<Result<App, String>> = OnceLock::new();

    /// The DLL: `OSPREY_NATIVE_APP_DLL`, or `app.dll` beside the host executable.
    fn path() -> Result<PathBuf, String> {
        if let Some(named) = std::env::var_os("OSPREY_NATIVE_APP_DLL") {
            return Ok(PathBuf::from(named));
        }
        let exe = std::env::current_exe()
            .map_err(|problem| format!("the host cannot find itself: {problem}"))?;
        Ok(exe.with_file_name("app.dll"))
    }

    fn symbol(module: *mut c_void, name: &str) -> Result<*mut c_void, String> {
        let text = std::ffi::CString::new(name).map_err(|_| format!("{name} holds a NUL byte"))?;
        // SAFETY: a live module handle and a NUL-terminated name.
        let found = unsafe { GetProcAddress(module, text.as_ptr()) };
        if found.is_null() {
            Err(format!("the application DLL exports no {name}"))
        } else {
            Ok(found)
        }
    }

    fn open_dll() -> Result<App, String> {
        use std::os::windows::ffi::OsStrExt;
        let path = path()?;
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: a NUL-terminated wide path; the module is never freed.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return Err(format!(
                "the application DLL {} did not load: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: each export has exactly the C signature the generated header declares for it.
        let app = unsafe {
            App {
                main: std::mem::transmute::<*mut c_void, Main>(symbol(module, "osprey_main")?),
                argument: std::mem::transmute::<*mut c_void, Argument>(symbol(
                    module,
                    "osprey_app_argument",
                )?),
                open: std::mem::transmute::<*mut c_void, Open>(symbol(module, "osprey_app_open")?),
                event: std::mem::transmute::<*mut c_void, Event>(symbol(
                    module,
                    "osprey_app_event",
                )?),
                version: std::mem::transmute::<*mut c_void, Version>(symbol(
                    module,
                    "osprey_host_abi_version",
                )?),
                bridge: &*symbol(module, "osprey_native_bridge")?.cast::<BridgeTable>(),
            }
        };
        // The one import this host answers; an application without it simply never asks.
        if let Ok(binder) = symbol(module, "osprey_import_native_slice") {
            // SAFETY: the binder stores the pointer in the DLL's slot; `native_slice` lives forever.
            unsafe {
                std::mem::transmute::<*mut c_void, Binder>(binder)(
                    crate::ffi::native_slice as *const c_void,
                )
            };
        }
        Ok(app)
    }

    /// Loads the application once; every later call reuses it or its failure.
    pub(crate) fn load() -> Result<(), String> {
        APP.get_or_init(open_dll)
            .as_ref()
            .map(|_| ())
            .map_err(Clone::clone)
    }

    fn app() -> Option<&'static App> {
        APP.get().and_then(|loaded| loaded.as_ref().ok())
    }

    /// What every entry answers before the application has loaded: a refusal, never a call.
    const NOT_LOADED: i32 = -1;

    pub(crate) unsafe fn osprey_main() -> i32 {
        // SAFETY: the caller upholds the export's contract.
        app().map_or(NOT_LOADED, |a| unsafe { (a.main)() })
    }
    pub(crate) unsafe fn osprey_app_argument(text: *const c_char) -> i64 {
        // SAFETY: as above.
        app().map_or(i64::from(NOT_LOADED), |a| unsafe { (a.argument)(text) })
    }
    pub(crate) unsafe fn osprey_app_open(
        now: i64,
        offset: i64,
        platform: *const c_char,
        home: *const c_char,
        dark: bool,
    ) -> i64 {
        // SAFETY: as above.
        app().map_or(i64::from(NOT_LOADED), |a| unsafe {
            (a.open)(now, offset, platform, home, dark)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn osprey_app_event(
        now: i64,
        kind: i64,
        ordinal: i64,
        words: *const c_char,
        detail: *const c_char,
        x: f64,
        y: f64,
        down: bool,
        shift: bool,
    ) -> i64 {
        // SAFETY: as above.
        app().map_or(i64::from(NOT_LOADED), |a| unsafe {
            (a.event)(now, kind, ordinal, words, detail, x, y, down, shift)
        })
    }
    pub(crate) unsafe fn osprey_host_abi_version() -> i32 {
        // SAFETY: as above.
        app().map_or(NOT_LOADED, |a| unsafe { (a.version)() })
    }
    pub(crate) unsafe fn osp_native_open(
        instance: *const Instance,
        fingerprint: *const c_char,
        bytes: i64,
        chunks: i64,
        owners: i64,
        out: *mut *mut Bridge,
    ) -> i32 {
        // SAFETY: as above.
        app().map_or(NOT_LOADED, |a| unsafe {
            (a.bridge.open)(instance, fingerprint, bytes, chunks, owners, out)
        })
    }
    pub(crate) unsafe fn osp_native_frame_commit(bridge: *mut Bridge, revision: *mut i64) -> i32 {
        // SAFETY: as above.
        app().map_or(NOT_LOADED, |a| unsafe {
            (a.bridge.frame_commit)(bridge, revision)
        })
    }
    pub(crate) unsafe fn osp_native_acquire(
        bridge: *mut Bridge,
        out: *mut Lease,
        revision: *mut i64,
    ) -> i32 {
        // SAFETY: as above.
        app().map_or(NOT_LOADED, |a| unsafe {
            (a.bridge.acquire)(bridge, out, revision)
        })
    }
    pub(crate) unsafe fn osp_native_child(
        bridge: *mut Bridge,
        lease: *const Lease,
        index: i64,
        out: *mut Lease,
    ) -> i32 {
        // SAFETY: as above.
        app().map_or(NOT_LOADED, |a| unsafe {
            (a.bridge.child)(bridge, lease, index, out)
        })
    }
    pub(crate) unsafe fn osp_native_length(
        bridge: *mut Bridge,
        lease: *const Lease,
        length: *mut i64,
    ) -> i32 {
        // SAFETY: as above.
        app().map_or(NOT_LOADED, |a| unsafe {
            (a.bridge.length)(bridge, lease, length)
        })
    }
    pub(crate) unsafe fn osp_native_view(
        bridge: *mut Bridge,
        lease: *const Lease,
        kind: i64,
        offset: i64,
        length: i64,
        out: *mut *const c_void,
    ) -> i32 {
        // SAFETY: as above.
        app().map_or(NOT_LOADED, |a| unsafe {
            (a.bridge.view)(bridge, lease, kind, offset, length, out)
        })
    }
    pub(crate) unsafe fn osp_native_release(bridge: *mut Bridge, lease: *const Lease) -> i32 {
        // SAFETY: as above.
        app().map_or(NOT_LOADED, |a| unsafe { (a.bridge.release)(bridge, lease) })
    }
    pub(crate) unsafe fn osp_native_frame_discard() {
        if let Some(a) = app() {
            // SAFETY: as above.
            unsafe { (a.bridge.frame_discard)() };
        }
    }
}
