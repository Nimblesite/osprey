//! Bounded process.stdin work uses host-held text slices, never command-line payloads.
use super::{number, process, text};
use crate::ffi;
use crate::rows::{Ask, Field};

fn one(pairs: &[String], key: &str) -> Result<String, String> {
    let values: Vec<_> = pairs
        .as_chunks::<2>()
        .0
        .iter()
        .filter(|pair| pair[0] == key)
        .collect();
    match values.as_slice() {
        [pair] => Ok(pair[1].clone()),
        _ => Err(format!("process.input requires exactly one {key}")),
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
    let pairs = arguments(ask)?;
    let input =
        ffi::get_slice(&one(&pairs, "stdinSlice")?).ok_or("Process input slice is missing")?;
    process::with_input(&request(ask, &pairs)?, input)
}
