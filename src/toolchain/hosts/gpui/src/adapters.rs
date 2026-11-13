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

use crate::events::Touch;
use crate::rows::Ask;

pub use platform::dispatch as dispatch_platform;

/// Answers one synchronous non-store capability.
pub fn answer(ask: &Ask) -> Result<String, String> {
    match ask.kind() {
        // [CAST-MOBILE-DELIVERY] iOS launches no process and has no GPUI path
        // prompt: its OS services answer run (discovery) and choose-file across
        // the bridge, on the caller's background executor.
        #[cfg(any(target_os = "ios", target_os = "android"))]
        "run" | "choose-file" | "host-call" => mobile::answer(ask),
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        "run" => process::run(ask),
        "list-directories" => files::list_directories(ask),
        "read-texts" => files::read_texts(ask),
        // Cancel arrives only after synchronous adapters already completed, so
        // nothing is in flight to stop and an empty result acknowledges the ticket.
        "cancel" => Ok(String::new()),
        kind => Err(format!("unsupported capability {kind}")),
    }
}

/// Wraps a synchronous capability result in its ticketed completion.
pub fn completion(ask: &Ask) -> Touch {
    Touch::completed(&ask.ticket, answer(ask))
}

/// The one shape of every missing-or-mistyped argument error.
fn message(ask: &Ask, name: &str) -> String {
    format!("{} has no valid {name}", ask.kind())
}

/// Reads a required text argument at argument position `field`.
pub(super) fn text(ask: &Ask, field: i64, name: &str) -> Result<String, String> {
    ask.fields
        .iter()
        .find(|argument| argument.field == field)
        .map(|argument| argument.text.clone())
        .ok_or_else(|| message(ask, name))
}

/// Reads a required nonnegative whole-number argument at position `field`.
pub(super) fn number(ask: &Ask, field: i64, name: &str) -> Result<u64, String> {
    let whole = ask
        .fields
        .iter()
        .find(|argument| argument.field == field)
        .map(|argument| argument.number)
        .ok_or_else(|| message(ask, name))?;
    u64::try_from(whole).map_err(|_| message(ask, name))
}
