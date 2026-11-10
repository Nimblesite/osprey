//! The hostile raw-TCP origin for `http_client_raw_origin.rs`: one thread per
//! connection plays its attack byte for byte (dripping, flooding or holding the
//! socket open) and records how long the client kept it. [HTTP-RESPONSE-FRAMING]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::Attack;

/// The hostile origin: a stop flag, and the per-attack connection durations.
pub(super) struct Origin {
    pub(super) stop_flag: Arc<AtomicBool>,
    pub(super) durations: Arc<Mutex<Vec<Duration>>>,
}
/// Bind and accept; every connection is its own thread. Bind failure
/// answers `None`, which the caller asserts on.
pub(super) fn spawn_origin(table: &'static [Attack]) -> Option<(Origin, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = listener.local_addr().ok()?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let durations = Arc::new(Mutex::new(vec![Duration::ZERO; table.len()]));
    let origin = Origin {
        stop_flag: Arc::clone(&stop),
        durations: Arc::clone(&durations),
    };
    let _ = thread::spawn(move || serve(&listener, table, &stop, &durations));
    Some((origin, port))
}

/// Accept until told to stop; a refused connection is never an error here.
fn serve(
    listener: &TcpListener,
    table: &'static [Attack],
    stop: &Arc<AtomicBool>,
    durations: &Arc<Mutex<Vec<Duration>>>,
) {
    let _ = listener.set_nonblocking(true);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let durations = Arc::clone(durations);
                let _ = thread::spawn(move || answer_connection(stream, table, &durations));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

/// Record how long attack `index` kept its connection.
fn record(durations: &Arc<Mutex<Vec<Duration>>>, index: usize, took: Duration) {
    if let Ok(mut slots) = durations.lock() {
        if let Some(slot) = slots.get_mut(index) {
            *slot = took;
        }
    }
}

/// One connection's thread: play its attack, then record its lifetime.
fn answer_connection(
    stream: TcpStream,
    table: &'static [Attack],
    durations: &Arc<Mutex<Vec<Duration>>>,
) {
    if let Some((index, took)) = play_attack(stream, table) {
        record(durations, index, took);
    }
}

/// Play one attack: answer `/a/<index>` with its bytes, hold or close, and
/// report the attack's index and the connection's lifetime. The stream may
/// inherit the listener's non-blocking mode; the script needs blocking I/O.
fn play_attack(mut stream: TcpStream, table: &'static [Attack]) -> Option<(usize, Duration)> {
    let _ = stream.set_nonblocking(false);
    let index = read_attack_index(&mut stream)?;
    let attack = table.get(index)?;
    let started = Instant::now();
    send_reply(&mut stream, attack);
    if attack.hold_open {
        hold_until_closed(&mut stream);
    }
    Some((index, started.elapsed()))
}

/// Read the request head and take the attack index from its `/a/<index>` path.
fn read_attack_index(stream: &mut TcpStream) -> Option<usize> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut head = Vec::new();
    let mut chunk = [0u8; 512];
    while head.len() < 8192 && !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => head.extend_from_slice(chunk.get(..read)?),
        }
    }
    let head = String::from_utf8_lossy(&head);
    head.split_whitespace()
        .nth(1)?
        .strip_prefix("/a/")?
        .parse()
        .ok()
}

/// Write the attack's bytes, optionally one at a time, then any filler; a
/// write error means the client has hung up.
fn send_reply(stream: &mut TcpStream, attack: &Attack) {
    if attack.drip_ms == 0 {
        let _ = stream.write_all(attack.reply);
    } else {
        for byte in attack.reply {
            if stream.write_all(std::slice::from_ref(byte)).is_err() {
                return; // the client has its refusal and left
            }
            thread::sleep(Duration::from_millis(attack.drip_ms));
        }
    }
    if let Some(total) = attack.fill_to {
        fill_with(stream, attack.reply.len(), total);
    }
}

/// The size-limit filler: static, so nothing huge lives on the stack.
const FILLER: &[u8] = &[b'x'; 65536];

/// Pad the reply with filler to a total size; a failed write means the client hung up.
fn fill_with(stream: &mut TcpStream, mut sent: usize, total: usize) {
    while sent < total {
        let take = (total - sent).min(FILLER.len());
        let Some(chunk) = FILLER.get(..take) else {
            break;
        };
        if stream.write_all(chunk).is_err() {
            break;
        }
        sent += take;
    }
}

/// Hold the socket open until the client disconnects.
fn hold_until_closed(stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut sink = [0u8; 64];
    let _ = stream.read(&mut sink);
}
