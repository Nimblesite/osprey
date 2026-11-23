//! Mechanical process, file and platform capabilities ([GPUI-HOST-CAPABILITIES]).

#[path = "../gpui/src/adapters/files.rs"]
mod files;
pub(crate) mod picker;
#[path = "../gpui/src/adapters/process.rs"]
pub(crate) mod process;
#[path = "../gpui/src/adapters/process_input.rs"]
mod process_input;
#[path = "../gpui/src/adapters/process_session.rs"]
mod process_session;
#[path = "../gpui/src/adapters/process_sessions.rs"]
mod process_sessions;
pub(crate) use process_sessions::Cleanup;

use crate::events::Touch;
use crate::rows::Ask;

include!("../gpui/src/adapters/answering.rs");
