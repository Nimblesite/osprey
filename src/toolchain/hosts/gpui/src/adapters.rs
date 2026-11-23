//! Mechanical process, file and platform capabilities ([GPUI-HOST-CAPABILITIES]).

mod files;
// [CAST-MOBILE-DELIVERY] the iOS OS bridge; its protocol layer also compiles for
// boundary tests, while the C calls themselves stay iOS-only. Mobile is unfinished,
// so CI leaves those tests out: `make -C src/toolchain/targets/tests casting-mobile-tools` runs them.
#[cfg(any(
    target_os = "ios",
    target_os = "android",
    all(test, feature = "mobile-bridge-tests")
))]
mod mobile;
mod platform;
// iOS launches no processes: its `run` capability is OS discovery across the bridge.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod process;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod process_input;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod process_session;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod process_sessions;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use process_sessions::Cleanup;

use crate::events::Touch;
use crate::rows::Ask;

pub use platform::dispatch as dispatch_platform;

// The capability dispatch and argument readers both hosts share; each host declares the modules it names.
include!("adapters/answering.rs");
