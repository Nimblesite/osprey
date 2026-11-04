//! The CPU backend against its error contract [PROF-ERRORS], and against a
//! scripted peer on a real socket for the replies a real program never sends.

use super::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::thread::JoinHandle;

#[test]
fn a_pid_with_no_process_is_not_found_and_a_live_one_is_not_osprey() {
    let cpu = OspreyCpu::new(Endpoints::new(Some(
        std::env::temp_dir().join("osprey-none"),
    )));
    assert_eq!(
        cpu.endpoint_of(u32::MAX - 7),
        Err(SamplerError::ProcessNotFound(u32::MAX - 7))
    );
    let own = std::process::id();
    assert_eq!(
        cpu.endpoint_of(own),
        Err(SamplerError::UnsupportedRuntime {
            pid: own,
            runtime: RUNTIME.to_owned()
        })
    );
}

/// A program killed by SIGKILL leaves its socket behind until the next start
/// sweeps it, so a refused connection names a dead pid as not found and only
/// a live one — the OS reused the pid — as not an Osprey program.
#[tokio::test]
async fn a_stale_socket_of_a_dead_program_is_not_found() {
    let dir = std::env::temp_dir().join(format!("osprey-cpu-stale-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap();
    let dead = u32::MAX - 7;
    drop(UnixListener::bind(dir.join(format!("{dead}.sock"))).unwrap()); // the file stays, unserved
    let cpu = OspreyCpu::new(Endpoints::new(Some(dir)));
    let attach = cpu
        .attach(&SamplerConfig {
            pid: dead,
            ..config()
        })
        .await;
    assert!(matches!(attach, Err(SamplerError::ProcessNotFound(pid)) if pid == dead));
}

#[test]
fn connection_failures_are_classified() {
    let refused_error = io::Error::from(io::ErrorKind::ConnectionRefused);
    let own = std::process::id();
    assert!(matches!(
        refused(own, &refused_error),
        SamplerError::UnsupportedRuntime { pid, .. } if pid == own
    ));
    assert_eq!(
        refused(u32::MAX - 7, &refused_error),
        SamplerError::ProcessNotFound(u32::MAX - 7)
    );
    let denied = io::Error::from(io::ErrorKind::PermissionDenied);
    assert!(
        matches!(refused(3, &denied), SamplerError::PermissionDenied(message) if message.contains("PID 3"))
    );
    let reset = io::Error::from(io::ErrorKind::ConnectionReset);
    assert!(matches!(refused(3, &reset), SamplerError::AttachFailed(_)));
}

const HELLO: &str = r#"{"kind":"hello","protocol":1,"runtime_version":"9.9.9","exe":"/bin/app"}"#;
const STARTED: &str = r#"{"kind":"cpu_started","images":[]}"#;

/// A peer serving `<dir>/1.sock`: for each request line, in order, it checks
/// the request and writes that step's replies.
fn scripted(name: &str, steps: Vec<(&'static str, Vec<String>)>) -> (OspreyCpu, JoinHandle<()>) {
    let dir = std::env::temp_dir().join(format!("osprey-cpu-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap();
    let listener = UnixListener::bind(dir.join("1.sock")).unwrap();
    let peer = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut lines = BufReader::new(stream.try_clone().unwrap()).lines();
        let mut out = stream;
        for (request, replies) in steps {
            let line = lines.next().unwrap().unwrap();
            assert!(line.starts_with(request), "expected {request}, got {line}");
            for reply in replies {
                writeln!(out, "{reply}").unwrap();
            }
        }
        drop(lines.next()); // hold the connection until the client closes it
    });
    (OspreyCpu::new(Endpoints::new(Some(dir))), peer)
}

fn config() -> SamplerConfig {
    SamplerConfig {
        pid: 1,
        sample_rate: 100,
        include_native: false,
        include_idle: false,
        duration: None,
    }
}

async fn refusal(name: &str, steps: Vec<(&'static str, Vec<String>)>) -> String {
    let (cpu, _peer) = scripted(name, steps);
    match cpu.attach(&config()).await {
        Ok(_) => panic!("{name}: attach must fail"),
        Err(SamplerError::AttachFailed(message)) => message,
        Err(other) => panic!("{name}: {other}"),
    }
}

#[tokio::test]
async fn a_program_speaking_another_protocol_is_refused() {
    let newer = HELLO.replace("\"protocol\":1", "\"protocol\":2");
    let message = refusal("protocol", vec![("hello", vec![newer])]).await;
    assert!(message.contains("protocol 2"), "{message}");
}

#[tokio::test]
async fn a_refused_cpu_start_names_the_endpoints_reason() {
    let busy = r#"{"kind":"error","code":"already_profiling","message":"busy"}"#.to_owned();
    let steps = vec![
        ("hello", vec![HELLO.to_owned()]),
        ("cpu_start 100", vec![busy]),
    ];
    assert_eq!(refusal("busy", steps).await, "already_profiling: busy");
}

#[tokio::test]
async fn replies_out_of_order_fail_the_attach() {
    let stopped = r#"{"kind":"cpu_stopped","reason":"exit"}"#.to_owned();
    let no_hello = refusal("nohello", vec![("hello", vec![stopped])]).await;
    assert!(no_hello.contains("hello got no answer"), "{no_hello}");
    let steps = vec![
        ("hello", vec![HELLO.to_owned()]),
        ("cpu_start", vec![HELLO.to_owned()]),
    ];
    let no_start = refusal("nostart", steps).await;
    assert!(no_start.contains("no cpu_started"), "{no_start}");
}

/// A session streams past stray messages, delivers its batch as one tick, and
/// ends when the stop the session asks for is answered.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_relays_batches_until_its_stop_is_answered() {
    let batch = r#"{"kind":"cpu_batch","threads":[{"fiber":0,"label":"main"}],"stacks":[[16]],"samples":[[1,0,0,0,1]]}"#;
    let stray = vec![
        r#"{"kind":"error","code":"unknown_command","message":"x"}"#.to_owned(),
        HELLO.to_owned(),
        STARTED.to_owned(),
        batch.to_owned(),
    ];
    let mut opening = vec![STARTED.to_owned()];
    opening.extend(stray);
    let stopped = vec![r#"{"kind":"cpu_stopped","reason":"requested"}"#.to_owned()];
    let steps = vec![
        ("hello", vec![HELLO.to_owned()]),
        ("cpu_start", opening),
        ("cpu_stop", stopped),
    ];
    let (cpu, peer) = scripted("relay", steps);
    let mut handle = cpu.attach(&config()).await.unwrap();
    assert_eq!(handle.runtime_version, "9.9.9");
    let tick = handle.receiver.recv().await.unwrap();
    assert_eq!(tick.samples.len(), 1);
    let mut late = 0;
    handle.stop(|_| late += 1).await;
    assert_eq!(late, 0);
    peer.join().unwrap();
}
