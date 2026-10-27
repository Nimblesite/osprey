//! Mechanical process, file and platform capabilities ([GPUI-HOST-CAPABILITIES]).

mod files;
mod platform;
mod process;

use serde_json::Value;

use crate::envelope::Request;
use crate::events;
use crate::host::now;

pub use platform::dispatch as dispatch_platform;

/// Answers one synchronous non-store capability.
pub fn answer(request: &Request) -> Result<Value, String> {
    match request.kind.as_str() {
        "run" => process::run(request),
        "list-directories" => files::list_directories(request),
        "read-texts" => files::read_texts(request),
        // Cancel arrives only after synchronous adapters already completed, so
        // nothing is in flight to stop and ok/null acknowledges the ticket.
        "cancel" => Ok(Value::Null),
        kind => Err(format!("unsupported capability {kind}")),
    }
}

/// Wraps a synchronous capability result in its ticketed completion.
pub fn completion(request: &Request) -> String {
    match answer(request) {
        Ok(result) => events::completed_ok(now(), &request.ticket, result),
        Err(problem) => events::completed_err(now(), &request.ticket, &problem),
    }
}

/// Reads a required string argument from the verbatim request fields.
fn field<'a>(request: &'a Request, name: &str) -> Result<&'a str, String> {
    request
        .fields
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{} has no valid {name}", request.kind))
}

/// Reads a required nonnegative integer argument.
fn number(request: &Request, name: &str) -> Result<u64, String> {
    request
        .fields
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{} has no valid {name}", request.kind))
}

/// Reads a required array containing only strings.
fn strings<'a>(request: &'a Request, name: &str) -> Result<Vec<&'a str>, String> {
    request
        .fields
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{} has no valid {name}", request.kind))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{} has no valid {name}", request.kind))
        })
        .collect()
}

/// Builds a request with unique test data without touching environment state.
#[cfg(test)]
fn request(kind: &str, fields: &[(&str, Value)]) -> Request {
    Request {
        ticket: format!("{kind}-test"),
        kind: kind.to_owned(),
        fields: serde_json::Map::from_iter(
            fields
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone())),
        ),
    }
}
