//! Bounded process.stdin work uses host-held text slices, never command-line payloads.
use super::{number, process, text};
use crate::ffi;
use crate::rows::{Ask, Field};

pub(super) fn one(pairs: &[String], key: &str) -> Result<String, String> {
    let values: Vec<_> = pairs
        .as_chunks::<2>()
        .0
        .iter()
        .filter(|pair| pair[0] == key)
        .collect();
    match values.as_slice() {
        [pair] => Ok(pair[1].clone()),
        _ => Err(format!("Process call requires exactly one {key}")),
    }
}

fn field(code: i64, value: String) -> Field {
    Field {
        field: code,
        number: 0,
        text: value,
    }
}

fn arguments(ask: &Ask) -> Result<Vec<String>, String> {
    if text(ask, 1, "namespace")? != "process" || text(ask, 2, "operation")? != "input" {
        return Err("Unsupported host call; expected process.input".to_owned());
    }
    let pairs = ask.texts(3);
    if !pairs.as_chunks::<2>().1.is_empty()
        || pairs
            .as_chunks::<2>()
            .0
            .iter()
            .any(|p| !matches!(p[0].as_str(), "executable" | "argv" | "stdinSlice"))
    {
        return Err("Invalid process.input argument pairs".to_owned());
    }
    Ok(pairs)
}

fn request(ask: &Ask, pairs: &[String]) -> Result<Ask, String> {
    let mut fields = vec![field(1, one(pairs, "executable")?)];
    fields.extend(
        pairs
            .as_chunks::<2>()
            .0
            .iter()
            .filter(|p| p[0] == "argv")
            .map(|p| field(2, p[1].clone())),
    );
    for (source, destination, name) in [(4, 3, "timeout"), (5, 4, "limit")] {
        fields.push(Field {
            field: destination,
            number: number(ask, source, name)? as i64,
            text: String::new(),
        });
    }
    fields.push(field(6, text(ask, 6, "slice")?));
    Ok(Ask {
        code: 1,
        ticket: ask.ticket.clone(),
        fields,
    })
}

/// Shares Run's process lifecycle, output limits and receipt; only stdin delivery differs.
pub fn answer(ask: &Ask) -> Result<String, String> {
    if text(ask, 1, "namespace")? == "process" && text(ask, 2, "operation")? != "input" {
        return super::process_sessions::answer(ask);
    }
    let pairs = arguments(ask)?;
    let input =
        ffi::get_slice(&one(&pairs, "stdinSlice")?).ok_or("Process input slice is missing")?;
    process::with_input(&request(ask, &pairs)?, input)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn call(operation: &str, pairs: &[&str]) -> Ask {
        let mut fields = vec![
            field(1, "process".to_owned()),
            field(2, operation.to_owned()),
            field(6, "session-public-output".to_owned()),
        ];
        fields.extend(pairs.iter().map(|value| field(3, (*value).to_owned())));
        fields.extend([(4, 1_000), (5, 1_000_000)].map(|(field, number)| Field {
            field,
            number,
            text: String::new(),
        }));
        Ask {
            code: 11,
            ticket: "session-public".to_owned(),
            fields,
        }
    }

    #[test]
    /// [SYNTH-HOST-STREAM-SESSION] Public handles reject bad shapes, close, and release slots.
    fn session_host_calls_have_bounded_ownership() -> Result<(), String> {
        let opened = answer(&call("session-open", &["executable", "/bin/cat"]))?;
        let value: serde_json::Value = serde_json::from_str(&opened).map_err(|e| e.to_string())?;
        let key = value["session"].as_str().ok_or("No session handle")?;
        for text in ["first", "second", "third"] {
            ffi::put_slice("session-public-input", text)?;
            let receipt = answer(&call(
                "session-exchange",
                &["session", key, "stdinSlice", "session-public-input"],
            ))?;
            assert!(receipt.contains("\"status\":0"));
            assert_eq!(
                ffi::get_slice("session-public-output").as_deref(),
                Some(text)
            );
        }
        assert!(answer(&call("session-exchange", &["session", key, "session", key])).is_err());
        assert!(answer(&call(
            "session-open",
            &["executable", "/bin/cat", "unknown", "x"]
        ))
        .is_err());
        assert!(answer(&call("session-close", &["session", key])).is_ok());
        assert!(answer(&call("session-close", &["session", key])).is_err());
        assert!(answer(&call(
            "session-exchange",
            &["session", key, "stdinSlice", "session-public-input"]
        ))
        .is_err());
        let nul = answer(&call(
            "session-open",
            &[
                "executable",
                "/bin/sh",
                "argv",
                "-c",
                "argv",
                "read line; printf '\\000\\n'; exec sleep 2",
            ],
        ))?;
        let nul: serde_json::Value = serde_json::from_str(&nul).map_err(|e| e.to_string())?;
        let key = nul["session"].as_str().ok_or("No NUL worker handle")?;
        assert!(answer(&call(
            "session-exchange",
            &["session", key, "stdinSlice", "session-public-input"]
        ))
        .is_err());
        assert!(
            answer(&call("session-close", &["session", key])).is_err(),
            "A reply that cannot cross the native text boundary must release its worker"
        );
        let cleanup = super::super::Cleanup;
        for _ in 0..16 {
            assert!(answer(&call("session-open", &["executable", "/bin/cat"])).is_ok());
        }
        assert!(answer(&call("session-open", &["executable", "/bin/cat"])).is_err());
        drop(cleanup);
        let cleanup = super::super::Cleanup;
        assert!(answer(&call("session-open", &["executable", "/bin/cat"])).is_ok());
        drop(cleanup);
        Ok(())
    }
}
