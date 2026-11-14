//! The iOS OS-capability boundary ([CAST-MOBILE-DELIVERY], [GPUI-HOST-CAPABILITIES]):
//! one serialized capability request crosses a C ABI to the Swift OS services on
//! the background executor, and one owned JSON answer crosses back and is decoded
//! whole into the existing `Value` before any slice is published, so a malformed
//! answer leaves the host untouched. The protocol layer is compiled for boundary
//! tests as well; only the C calls themselves are iOS-only, keeping desktop tests
//! free of native OS symbols.

use serde_json::{Map, Value};

use crate::ffi;
use crate::rows::Ask;

/// The Swift OS-service boundary: one JSON request in, one owned answer out that
/// is copied and freed exactly once before decoding continues.
#[cfg(target_os = "ios")]
mod bridge {
    use std::ffi::{c_char, CStr, CString};

    extern "C" {
        /// Answers one serialized capability request; a nil answer reports failure.
        fn osprey_gpui_capability(request: *const c_char) -> *mut c_char;
        /// Releases one owned answer the call returned.
        fn osprey_gpui_capability_free(answer: *mut c_char);
    }

    /// Calls the OS bridge with one serialized request and copies its owned answer.
    pub(super) fn call(request: &str) -> Result<String, String> {
        let held = CString::new(request).map_err(|_| {
            "The capability request contained a NUL byte, which no C string can carry".to_owned()
        })?;
        // SAFETY: the request outlives the call and the answer is owned until freed.
        let answer = unsafe { osprey_gpui_capability(held.as_ptr()) };
        if answer.is_null() {
            return Err("The OS capability bridge returned no answer".to_owned());
        }
        // SAFETY: a non-null answer is NUL-terminated; it is copied in full and its
        // single owned reference is released before the copy is decoded, so every
        // later failure still frees exactly once.
        let copied = unsafe { CStr::from_ptr(answer) }.to_bytes().to_vec();
        // SAFETY: exactly one release for the one reference the call returned.
        unsafe { osprey_gpui_capability_free(answer) };
        String::from_utf8(copied).map_err(|_| "The OS capability answer is not UTF-8".to_owned())
    }
}

/// The Android OS-service boundary: `CastingActivity.answer` over JNI, one JSON
/// request in and one JSON answer out, on this background thread.
#[cfg(target_os = "android")]
mod bridge {
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;

    pub(super) fn call(request: &str) -> Result<String, String> {
        let app = crate::host::ANDROID_APP
            .get()
            .ok_or("No Android activity answers capabilities")?;
        // SAFETY: the VM and activity pointers stay valid for the process, and the
        // activity is a global reference this borrow never deletes.
        let (vm, activity) = unsafe {
            (
                JavaVM::from_raw(app.vm_as_ptr().cast()).map_err(|error| error.to_string())?,
                JObject::from_raw(app.activity_as_ptr().cast()),
            )
        };
        let mut env = vm
            .attach_current_thread()
            .map_err(|error| error.to_string())?;
        let input = env.new_string(request).map_err(|error| error.to_string())?;
        let called = env.call_method(
            &activity,
            "answer",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&input)],
        );
        let answer = match called.and_then(|value| value.l()) {
            Ok(answer) => JString::from(answer),
            Err(error) => {
                let _ = env.exception_clear();
                return Err(format!("The OS capability bridge failed: {error}"));
            }
        };
        let text = env.get_string(&answer).map_err(|error| error.to_string())?;
        Ok(text.into())
    }
}

/// Answers one capability through the OS bridge ([CAST-MOBILE-DELIVERY]); the
/// caller has already moved this call onto the background executor.
#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) fn answer(ask: &Ask) -> Result<String, String> {
    bridge::call(&encoded(ask))
        .and_then(|raw| applied(&raw))
        .map(result_text)
}

/// The completion's result text: an empty string for a null result, the document otherwise.
fn result_text(result: Value) -> String {
    match result {
        Value::Null => String::new(),
        result => result.to_string(),
    }
}

/// Serializes one ask — its arguments by position under the bridge's own names, plus
/// the kind and ticket — so the OS services see exactly what the application asked for.
fn encoded(ask: &Ask) -> String {
    let mut members = fields(ask);
    members.insert("kind".to_owned(), Value::String(ask.kind().to_owned()));
    members.insert("ticket".to_owned(), Value::String(ask.ticket.clone()));
    Value::Object(members).to_string()
}

/// The bridge's named request fields, read from the ask's argument positions.
fn fields(ask: &Ask) -> Map<String, Value> {
    let mut members = Map::new();
    match ask.kind() {
        "run" => {
            members.insert("executable".to_owned(), Value::String(ask.text(1)));
            members.insert("arguments".to_owned(), texts(ask, 2));
            members.insert("timeout".to_owned(), Value::from(ask.number(3)));
            members.insert("limit".to_owned(), Value::from(ask.number(4)));
            members.insert("plist".to_owned(), Value::Bool(ask.flag(5)));
            members.insert("slice".to_owned(), Value::String(ask.text(6)));
        }
        "choose-file" => {
            members.insert("types".to_owned(), texts(ask, 1));
            members.insert("limit".to_owned(), Value::from(ask.number(2)));
            members.insert("slice".to_owned(), Value::String(ask.text(3)));
        }
        "host-call" => {
            members.insert("namespace".to_owned(), Value::String(ask.text(1)));
            members.insert("operation".to_owned(), Value::String(ask.text(2)));
            members.insert("arguments".to_owned(), pairs(ask, 3));
            members.insert("timeout".to_owned(), Value::from(ask.number(4)));
            members.insert("limit".to_owned(), Value::from(ask.number(5)));
            members.insert("slice".to_owned(), Value::String(ask.text(6)));
        }
        _ => {}
    }
    members
}

/// A list argument as the bridge's JSON array.
fn texts(ask: &Ask, field: i64) -> Value {
    Value::Array(ask.texts(field).into_iter().map(Value::String).collect())
}

/// A pair argument as the bridge's JSON object.
fn pairs(ask: &Ask, field: i64) -> Value {
    Value::Object(
        ask.pairs(field)
            .into_iter()
            .map(|(name, value)| (name, Value::String(value)))
            .collect(),
    )
}

/// Applies one whole bridge answer: a validated success publishes its slices and
/// returns the result; a failure or malformed answer reports a typed error.
fn applied(answer: &str) -> Result<Value, String> {
    let reply: Value = serde_json::from_str(answer)
        .map_err(|problem| format!("The OS capability answer could not be decoded: {problem}"))?;
    match reply.get("ok").and_then(Value::as_bool) {
        Some(true) => succeeded(&reply),
        Some(false) => failed(&reply),
        _ => Err("The OS capability answer has no valid ok".to_owned()),
    }
}

/// Validates one success whole, then publishes every slice it carried.
fn succeeded(reply: &Value) -> Result<Value, String> {
    let Some(result) = reply.get("result").filter(|result| result.is_object()) else {
        return Err("The OS capability answer has no valid result".to_owned());
    };
    let decoded = slices(reply)?;
    decoded.iter().try_for_each(carried)?;
    decoded
        .iter()
        .try_for_each(|slice| ffi::put_slice(&slice.key, &slice.text))?;
    Ok(result.clone())
}

/// Reads the typed boundary error of one failure.
fn failed(reply: &Value) -> Result<Value, String> {
    let error = reply
        .get("error")
        .and_then(Value::as_str)
        .filter(|error| !error.trim().is_empty())
        .ok_or_else(|| "The OS capability failure has no valid error".to_owned())?;
    Err(error.to_owned())
}

/// Decodes the required slice array whole; one malformed entry refuses the answer.
fn slices(reply: &Value) -> Result<Vec<Slice>, String> {
    reply
        .get("slices")
        .and_then(Value::as_array)
        .ok_or_else(|| "The OS capability answer has no valid slices".to_owned())?
        .iter()
        .map(entry)
        .collect()
}

/// One host-held slice the bridge sends back.
struct Slice {
    key: String,
    text: String,
}

/// Decodes one slice entry: a nonempty string key and a string text.
fn entry(item: &Value) -> Result<Slice, String> {
    let key = item
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| "The OS capability answer has a slice with no valid key".to_owned())?;
    let text = item
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "The OS capability answer has a slice with no valid text".to_owned())?;
    Ok(Slice {
        key: key.to_owned(),
        text: text.to_owned(),
    })
}

/// A slice whose text cannot cross the C boundary refuses the whole answer first,
/// so no partially publishable reply ever reaches `put_slice`.
fn carried(slice: &Slice) -> Result<(), String> {
    match slice.text.contains('\0') {
        true => Err("The OS capability answer has slice text a C string cannot carry".to_owned()),
        false => Ok(()),
    }
}

/// Protocol isolation tests ([CAST-MOBILE-DELIVERY]): every case drives the
/// boundary with a raw answer string, so no test needs the iOS OS services or
/// their C symbols.
#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{applied, encoded, result_text};
    use crate::ffi;
    use crate::rows::{Ask, Field};

    /// One ask with an explicit ticket and argument rows.
    fn request(code: i64, ticket: &str, fields: &[(i64, i64, &str)]) -> Ask {
        Ask {
            code,
            ticket: ticket.to_owned(),
            fields: fields
                .iter()
                .map(|(field, number, text)| Field {
                    field: *field,
                    number: *number,
                    text: (*text).to_owned(),
                })
                .collect(),
        }
    }

    /// The reply the boundary sends for one answer, as its own JSON wire form.
    fn wire(reply: Value) -> String {
        reply.to_string()
    }

    /// The refusal for one malformed answer, or a message saying it was accepted.
    fn refusal(answer: &str) -> String {
        match applied(answer) {
            Ok(result) => format!("accepted the malformed answer {answer} as {result}"),
            Err(problem) => problem,
        }
    }

    /// Both bridged capabilities serialize their kind, ticket and every argument,
    /// and an argument cannot shadow either protocol member.
    #[test]
    fn bridged_requests_carry_kind_ticket_and_every_field() {
        let discover = request(
            1,
            "discover",
            &[
                (6, 0, "cast-browse"),
                (3, 5000, ""),
                (4, 65536, ""),
                (1, 0, "/usr/bin/dns-sd"),
                (2, 0, "-B"),
                (2, 0, "_googlecast._tcp"),
            ],
        );
        let sent = applied(&encoded(&discover))
            .expect_err("a serialized request is not a boundary answer");
        assert!(sent.contains("no valid ok"), "{sent}");
        let sent: Value = serde_json::from_str(&encoded(&discover)).expect("JSON serialization");
        assert_eq!(sent["kind"], "run");
        assert_eq!(sent["ticket"], "discover");
        assert_eq!(sent["slice"], "cast-browse");
        assert_eq!(sent["timeout"], 5000);
        assert_eq!(sent["limit"], 65536);
        assert_eq!(sent["executable"], "/usr/bin/dns-sd");
        assert_eq!(sent["arguments"][0], "-B");
        assert_eq!(sent["arguments"][1], "_googlecast._tcp");

        let pick = request(
            13,
            "pick-media",
            &[
                (1, 0, "txt"),
                (1, 0, "md"),
                (2, 4096, ""),
                (3, 0, "chosen-media"),
            ],
        );
        let sent: Value = serde_json::from_str(&encoded(&pick)).expect("JSON serialization");
        assert_eq!(sent["kind"], "choose-file");
        assert_eq!(sent["ticket"], "pick-media");
        assert_eq!(sent["types"][0], "txt");
        assert_eq!(sent["types"][1], "md");
        assert_eq!(sent["limit"], 4096);
        assert_eq!(sent["slice"], "chosen-media");

        let shadowed = request(1, "discover", &[(1, 0, "reveal"), (2, 0, "x")]);
        let sent: Value = serde_json::from_str(&encoded(&shadowed)).expect("JSON serialization");
        assert_eq!(sent["kind"], "run");
        assert_eq!(sent["ticket"], "discover");
    }

    /// A valid success publishes every slice and returns its result object.
    #[test]
    fn valid_success_answers_publish_their_slices_and_return_the_result() {
        let discovery = wire(json!({
            "ok": true,
            "result": {"status": 0, "stderr": ""},
            "slices": [
                {"key": "mobile-success-browse", "text": "_googlecast._tcp"},
                {"key": "mobile-success-extra", "text": "second slice"}
            ]
        }));
        let result = applied(&discovery).expect("a valid answer is accepted");
        assert_eq!(result, json!({"status": 0, "stderr": ""}));
        assert_eq!(
            result_text(result.clone()),
            json!({"status": 0, "stderr": ""}).to_string()
        );
        assert_eq!(result_text(Value::Null), String::new());
        assert_eq!(
            ffi::get_slice("mobile-success-browse").as_deref(),
            Some("_googlecast._tcp")
        );
        assert_eq!(
            ffi::get_slice("mobile-success-extra").as_deref(),
            Some("second slice")
        );

        let picked = wire(json!({
            "ok": true,
            "result": {"cancelled": false, "path": "/Documents/ChosenMedia/notes.txt"},
            "slices": []
        }));
        let result = applied(&picked).expect("a valid answer is accepted");
        assert_eq!(result["cancelled"], false);
        assert_eq!(result["path"], "/Documents/ChosenMedia/notes.txt");

        let cancelled = wire(json!({
            "ok": true,
            "result": {"cancelled": true, "path": ""},
            "slices": []
        }));
        let result = applied(&cancelled).expect("a valid answer is accepted");
        assert_eq!(result["cancelled"], true);
        assert_eq!(result["path"], "");
    }

    /// Unicode keys, texts and results cross the boundary verbatim.
    #[test]
    fn unicode_answers_cross_the_boundary_verbatim() {
        let transcript = "«Découverte» — 中文キャスト 🎬 line\nand a tab\t";
        let discovery = wire(json!({
            "ok": true,
            "result": {"label": "π✓"},
            "slices": [{"key": "mobile-unicode-π", "text": transcript}]
        }));
        let result = applied(&discovery).expect("a valid answer is accepted");
        assert_eq!(result["label"], "π✓");
        assert_eq!(
            ffi::get_slice("mobile-unicode-π").as_deref(),
            Some(transcript)
        );

        let failure = wire(json!({"ok": false, "error": "Impossible d'ouvrir «medias»"}));
        let problem = applied(&failure).expect_err("a failure is refused");
        assert_eq!(problem, "Impossible d'ouvrir «medias»");
    }

    /// Typed failures report their own error text, never an invented one.
    #[test]
    fn failure_answers_report_the_typed_error() {
        for error in [
            "Timed out after 600000 ms",
            "A file picker is already open",
            "Mobile casting does not execute processes",
            "Cannot read the chosen file",
        ] {
            let answer = wire(json!({"ok": false, "error": error}));
            let problem = applied(&answer).expect_err("a failure is refused");
            assert_eq!(problem, error);
        }
    }

    /// Undecodable, missing and wrongly typed members are refused by name.
    #[test]
    fn malformed_answers_are_refused_by_name() {
        const CASES: &[(&str, &str)] = &[
            ("not json", "could not be decoded"),
            ("[]", "no valid ok"),
            ("\"ok\"", "no valid ok"),
            ("5", "no valid ok"),
            ("null", "no valid ok"),
            ("{}", "no valid ok"),
            (r#"{"ok":"true"}"#, "no valid ok"),
            (r#"{"ok":1}"#, "no valid ok"),
            (r#"{"ok":null}"#, "no valid ok"),
            (r#"{"ok":true}"#, "no valid result"),
            (r#"{"ok":true,"result":"x"}"#, "no valid result"),
            (r#"{"ok":true,"result":[1]}"#, "no valid result"),
            (r#"{"ok":true,"result":5}"#, "no valid result"),
            (r#"{"ok":true,"result":null}"#, "no valid result"),
            (r#"{"ok":true,"result":{}}"#, "no valid slices"),
            (r#"{"ok":true,"result":{},"slices":"x"}"#, "no valid slices"),
            (r#"{"ok":true,"result":{},"slices":{}}"#, "no valid slices"),
            (r#"{"ok":true,"result":{},"slices":[5]}"#, "no valid key"),
            (r#"{"ok":true,"result":{},"slices":["x"]}"#, "no valid key"),
            (r#"{"ok":true,"result":{},"slices":[{}]}"#, "no valid key"),
            (
                r#"{"ok":true,"result":{},"slices":[{"key":null}]}"#,
                "no valid key",
            ),
            (
                r#"{"ok":true,"result":{},"slices":[{"key":5}]}"#,
                "no valid key",
            ),
            (
                r#"{"ok":true,"result":{},"slices":[{"key":""}]}"#,
                "no valid key",
            ),
            (
                r#"{"ok":true,"result":{},"slices":[{"key":"k"}]}"#,
                "no valid text",
            ),
            (
                r#"{"ok":true,"result":{},"slices":[{"key":"k","text":2}]}"#,
                "no valid text",
            ),
            (
                r#"{"ok":true,"result":{},"slices":[{"key":"k","text":null}]}"#,
                "no valid text",
            ),
            (r#"{"ok":false}"#, "no valid error"),
            (r#"{"ok":false,"error":5}"#, "no valid error"),
            (r#"{"ok":false,"error":{}}"#, "no valid error"),
            (r#"{"ok":false,"error":null}"#, "no valid error"),
            (r#"{"ok":false,"error":""}"#, "no valid error"),
        ];
        for (answer, wanted) in CASES {
            let problem = refusal(answer);
            assert!(problem.contains(wanted), "{answer}: {problem}");
        }
    }

    /// One malformed slice refuses the whole answer before anything is published.
    #[test]
    fn a_malformed_slice_publishes_nothing_from_the_same_answer() {
        let broken_text = wire(json!({
            "ok": true,
            "result": {},
            "slices": [
                {"key": "mobile-partial-first", "text": "first"},
                {"key": "mobile-partial-second", "text": 7}
            ]
        }));
        let problem = applied(&broken_text).expect_err("a malformed slice is refused");
        assert!(problem.contains("no valid text"), "{problem}");
        assert_eq!(ffi::get_slice("mobile-partial-first"), None);

        let nul_text = wire(json!({
            "ok": true,
            "result": {},
            "slices": [{"key": "mobile-partial-nul", "text": "before\u{0}after"}]
        }));
        let problem = applied(&nul_text).expect_err("unpublishable slice text is refused");
        assert!(problem.contains("C string"), "{problem}");
        assert_eq!(ffi::get_slice("mobile-partial-nul"), None);

        let missing_result = wire(json!({
            "ok": true,
            "slices": [{"key": "mobile-partial-unresulted", "text": "x"}]
        }));
        let problem = applied(&missing_result).expect_err("a missing result is refused");
        assert!(problem.contains("no valid result"), "{problem}");
        assert_eq!(ffi::get_slice("mobile-partial-unresulted"), None);
    }
}
