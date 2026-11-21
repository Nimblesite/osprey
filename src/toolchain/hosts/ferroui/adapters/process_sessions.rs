//! Desktop ownership and bounded handles for [SYNTH-HOST-STREAM-SESSION].
use super::{number, process_input::one, process_session::Session, text};
use crate::{ffi, rows::Ask};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

type Held = Arc<Mutex<Session>>;
static SESSIONS: OnceLock<Mutex<HashMap<String, Held>>> = OnceLock::new();
static SERIAL: AtomicU64 = AtomicU64::new(1);

fn sessions() -> &'static Mutex<HashMap<String, Held>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pairs(ask: &Ask, allowed: &[&str]) -> Result<Vec<String>, String> {
    let pairs = ask.texts(3);
    if !pairs.as_chunks::<2>().1.is_empty()
        || pairs
            .as_chunks::<2>()
            .0
            .iter()
            .any(|pair| !allowed.contains(&pair[0].as_str()))
    {
        return Err("Invalid process session argument pairs".to_owned());
    }
    Ok(pairs)
}

fn open(ask: &Ask) -> Result<String, String> {
    let pairs = pairs(ask, &["executable", "argv"])?;
    let executable = one(&pairs, "executable")?;
    let args: Vec<_> = pairs
        .as_chunks::<2>()
        .0
        .iter()
        .filter(|p| p[0] == "argv")
        .map(|p| p[1].clone())
        .collect();
    let mut registry = sessions()
        .lock()
        .map_err(|_| "Session registry unavailable")?;
    if registry.len() >= 16 {
        return Err("All 16 process session slots are occupied".to_owned());
    }
    let session = Session::open(&executable, &args)?;
    let key = format!("process-{}", SERIAL.fetch_add(1, Ordering::Relaxed));
    registry.insert(key.clone(), Arc::new(Mutex::new(session)));
    Ok(json!({"status": 0, "session": key}).to_string())
}

fn exchange(ask: &Ask) -> Result<String, String> {
    let pairs = pairs(ask, &["session", "stdinSlice"])?;
    let key = one(&pairs, "session")?;
    let input =
        ffi::get_slice(&one(&pairs, "stdinSlice")?).ok_or("Session input slice is missing")?;
    let output = text(ask, 6, "slice")?;
    let timeout = number(ask, 4, "timeout")?;
    let limit = number(ask, 5, "limit")?;
    let held = sessions()
        .lock()
        .map_err(|_| "Session registry unavailable")?
        .get(&key)
        .cloned()
        .ok_or("Unknown process session")?;
    let mut session = held.try_lock().map_err(|_| "Process session is busy")?;
    let result = session.exchange(&input, timeout, limit).and_then(|reply| {
        ffi::put_slice(&output, &reply)?;
        Ok(json!({"status": 0, "stderr": session.diagnostics()}).to_string())
    });
    if result.is_err() {
        session.stop();
        sessions()
            .lock()
            .map_err(|_| "Session registry unavailable")?
            .remove(&key);
    }
    result
}

fn close(ask: &Ask) -> Result<String, String> {
    let key = one(&pairs(ask, &["session"])?, "session")?;
    let mut registry = sessions()
        .lock()
        .map_err(|_| "Session registry unavailable")?;
    let held = registry.get(&key).ok_or("Unknown process session")?;
    let guard = held.try_lock().map_err(|_| "Process session is busy")?;
    drop(guard);
    registry.remove(&key);
    Ok(json!({"status": 0}).to_string())
}

pub(super) fn answer(ask: &Ask) -> Result<String, String> {
    match text(ask, 2, "operation")?.as_str() {
        "session-open" => open(ask),
        "session-exchange" => exchange(ask),
        "session-close" => close(ask),
        _ => Err("Unsupported process operation".to_owned()),
    }
}

/// Entry-point guard reaps retained workers for smoke, headless and windowed exits.
pub(crate) struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        // Even a poisoned registry must release its owned children on shutdown.
        let held = match sessions().lock() {
            Ok(mut registry) => std::mem::take(&mut *registry),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        for session in held.into_values() {
            match session.lock() {
                Ok(mut session) => session.stop(),
                Err(poisoned) => poisoned.into_inner().stop(),
            }
        }
    }
}
