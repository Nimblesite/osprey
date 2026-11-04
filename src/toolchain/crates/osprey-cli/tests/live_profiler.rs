//! Live-attach CPU profiling of a real Osprey program through lspkit's shared
//! profiler [PROF-ATTACH-ENDPOINT] [PROF-ATTACH-PROTOCOL] [PROF-LSP-RUNTIME].
//!
//! No mocks: this build's `osprey` compiles the fixture with debug info, the
//! program runs as its own process with its own endpoint, and the tests drive
//! `ProfilerService` over its wire methods exactly as the language server
//! does — the same contract lspkit's .NET suite pins for its runtime.
// The endpoint and its Unix socket are POSIX-only [PROF-ATTACH-ENDPOINT].
#![cfg(unix)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed setup step must fail the end-to-end test that needed it"
)]

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use lsp_profiler::wire::methods;
use lsp_profiler::{ProfilerConfig, ProfilerEvents, ProfilerService};
use lsp_profiler_osprey::{OspreyConfig, OspreyRuntime};
use serde_json::{json, Value};

#[path = "cases/live_fixture.rs"]
mod live_fixture;

use live_fixture::{
    listed_samples, listed_session, Fixture, Flavor, HOT_FUNCTION, POLL, RATE_HZ,
    RECORDING_DEADLINE,
};

#[derive(Debug)]
struct Silent;

#[async_trait]
impl ProfilerEvents for Silent {
    async fn notify(&self, _method: &'static str, _params: Value) {}
}

/// Every notification the service sends, in order.
#[derive(Debug, Default)]
struct Recorder(std::sync::Mutex<Vec<(&'static str, Value)>>);

#[async_trait]
impl ProfilerEvents for Recorder {
    async fn notify(&self, method: &'static str, params: Value) {
        if let Ok(mut sent) = self.0.lock() {
            sent.push((method, params));
        }
    }
}

impl Recorder {
    /// The `top_function` of every progress notification sent so far.
    fn tops(&self) -> Vec<String> {
        let sent = self.0.lock().map(|sent| sent.clone()).unwrap_or_default();
        sent.iter()
            .filter(|(method, _)| *method == methods::CPU_PROGRESS)
            .filter_map(|(_, params)| {
                params
                    .get("top_function")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect()
    }

    /// The `sample_count` of the latest progress notification; 0 before the first.
    fn progress_samples(&self) -> i64 {
        let sent = self.0.lock().map(|sent| sent.clone()).unwrap_or_default();
        sent.iter()
            .rev()
            .find(|(method, _)| *method == methods::CPU_PROGRESS)
            .and_then(|(_, params)| params.get("sample_count").and_then(Value::as_i64))
            .unwrap_or(0)
    }
}

/// The shared service with only the Osprey runtime, reading `fixture`'s
/// endpoint directory and writing exports under its scratch directory.
fn service(fixture: &Fixture) -> ProfilerService {
    service_with(fixture, Arc::new(Silent))
}

fn service_with(fixture: &Fixture, events: Arc<dyn ProfilerEvents>) -> ProfilerService {
    let runtime = OspreyRuntime::new(OspreyConfig {
        socket_dir: Some(fixture.socket_dir.clone()),
    });
    let config = ProfilerConfig {
        output_dir: fixture.tmp.join("out"),
        ..ProfilerConfig::default()
    };
    ProfilerService::new(vec![Arc::new(runtime)], config, events)
}

async fn call(service: &ProfilerService, method: &str, params: Value) -> Value {
    service
        .handle(method, params)
        .await
        .unwrap_or_else(|error| panic!("{method} failed: {error}"))
}

async fn error_code(service: &ProfilerService, method: &str, params: Value) -> i64 {
    match service.handle(method, params).await {
        Ok(value) => panic!("{method} must be refused, got {value}"),
        Err(error) => error.code(),
    }
}

/// Start a CPU session on the fixture, returning its `{ session_id }`.
async fn start_session(service: &ProfilerService, fixture: &Fixture, params: Value) -> Value {
    let mut start = json!({ "pid": fixture.pid, "sample_rate": RATE_HZ });
    if let (Some(start), Some(extra)) = (start.as_object_mut(), params.as_object()) {
        start.extend(extra.clone());
    }
    json!({ "session_id": call(service, methods::CPU_START, start).await.get("session_id") })
}

/// Record `enough` samples of the fixture, then stop and return the report.
async fn profile(
    service: &ProfilerService,
    fixture: &Fixture,
    params: Value,
    enough: i64,
) -> Value {
    let session = start_session(service, fixture, params).await;
    await_samples(service, &session, enough).await;
    call(service, methods::CPU_STOP, session).await
}

/// Wait until `session` has recorded `enough` samples, by its live count in `cpu.list`.
async fn await_samples(service: &ProfilerService, session: &Value, enough: i64) {
    await_listing(
        service,
        session,
        |listed| listed_samples(listed) >= enough,
        "the samples",
    )
    .await;
}

/// Poll `cpu.list` until `session`'s entry (`None` once it has ended) satisfies `until`.
async fn await_listing(
    service: &ProfilerService,
    session: &Value,
    until: impl Fn(Option<&Value>) -> bool,
    what: &str,
) {
    let list = || call(service, methods::CPU_LIST, json!({}));
    drop(poll(what, list, |list| until(listed_session(list, session))).await);
}

/// Re-read `state` until `holds` accepts it. Past [`RECORDING_DEADLINE`] the
/// test fails with `what` and the last state read.
async fn poll<T: std::fmt::Debug, F: Future<Output = T>>(
    what: &str,
    mut state: impl FnMut() -> F,
    holds: impl Fn(&T) -> bool,
) -> T {
    let deadline = Instant::now() + RECORDING_DEADLINE;
    loop {
        let current = state().await;
        if holds(&current) {
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "{what} not reached in {RECORDING_DEADLINE:?}: {current:?}"
        );
        tokio::time::sleep(POLL).await;
    }
}

fn hot_functions(report: &Value) -> &[Value] {
    report
        .get("hot_functions")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn samples(report: &Value) -> i64 {
    report
        .get("total_samples")
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_program_is_listed_as_an_attachable_osprey_process() {
    let fixture = Fixture::start("lp-list", Flavor::Default);
    let service = service(&fixture);
    let list = call(
        &service,
        methods::PROCESSES,
        json!({ "workspace_roots": [] }),
    )
    .await;
    let entry = list
        .get("processes")
        .and_then(Value::as_array)
        .and_then(|all| {
            all.iter()
                .find(|p| p.get("pid") == Some(&json!(fixture.pid)))
        })
        .unwrap_or_else(|| panic!("PID {} missing from {list}", fixture.pid));
    assert_eq!(entry.get("runtime"), Some(&json!("osprey")), "{entry}");
    assert_eq!(entry.get("attachable"), Some(&json!(true)), "{entry}");
    assert_eq!(entry.get("entry"), Some(&json!("live_attach")), "{entry}");
    let version = entry
        .get("runtime_version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(!version.is_empty(), "{entry}");
    let started = call(&service, methods::CPU_START, json!({ "pid": fixture.pid })).await;
    assert_eq!(
        started.get("runtime_version").and_then(Value::as_str),
        Some(version)
    );
    drop(
        call(
            &service,
            methods::CPU_STOP,
            json!({ "session_id": started.get("session_id") }),
        )
        .await,
    );
    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cpu_profiling_finds_the_hot_function_and_its_lines() {
    finds_the_hot_function_and_its_lines(Flavor::Default, "lp-cpu").await;
}

// The ML twin is attributed to its own .ospml source, line for line.
#[tokio::test(flavor = "multi_thread")]
async fn cpu_profiling_finds_the_ml_twins_hot_function_and_its_lines() {
    finds_the_hot_function_and_its_lines(Flavor::Ml, "lp-cpu-ml").await;
}

async fn finds_the_hot_function_and_its_lines(flavor: Flavor, name: &str) {
    let fixture = Fixture::start(name, flavor);
    let service = service(&fixture);
    let report = profile(&service, &fixture, json!({}), 3 * RATE_HZ).await;
    assert!(samples(&report) > 100, "{report}");
    let spin = hot_functions(&report)
        .iter()
        .find(|f| f.get("name") == Some(&json!(HOT_FUNCTION)))
        .unwrap_or_else(|| panic!("{HOT_FUNCTION} absent: {report}"));
    let file = spin.get("file").and_then(Value::as_str).unwrap_or_default();
    assert!(file.ends_with(flavor.source()), "{spin}");
    let share = spin
        .get("percentage")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    assert!(
        share > 50.0,
        "{HOT_FUNCTION} owns the busy main thread, {share}%: {report}"
    );
    let lines = report
        .get("hot_lines")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    assert!(
        lines
            .iter()
            .any(|l| l.get("file").and_then(Value::as_str) == Some(file)
                && flavor
                    .spin_lines()
                    .contains(&l.get("line").and_then(Value::as_i64).unwrap_or(0))),
        "a hot line inside {HOT_FUNCTION}: {report}"
    );
    for key in ["speedscope_path", "flamegraph_path", "cpuprofile_path"] {
        let path = report
            .pointer(&format!("/artifacts/{key}"))
            .and_then(Value::as_str);
        assert!(
            path.is_some_and(|p| Path::new(p).is_file()),
            "{key} not written: {report}"
        );
    }
    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn native_frames_appear_only_on_request() {
    native_frames_only_on_request(Flavor::Default, "lp-native").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ml_twins_native_frames_appear_only_on_request() {
    native_frames_only_on_request(Flavor::Ml, "lp-native-ml").await;
}

async fn native_frames_only_on_request(flavor: Flavor, name: &str) {
    let fixture = Fixture::start(name, flavor);
    let service = service(&fixture);
    // A native frame is any frame outside Osprey source (.osp or .ospml):
    // unlocated on a host without system debug info, located in libc
    // (`./stdio-common/...`) on one with it, as on the Linux runners.
    let native = |report: &Value| {
        hot_functions(report).iter().any(|f| {
            f.get("file").and_then(Value::as_str).is_none_or(|file| {
                Path::new(file)
                    .extension()
                    .is_none_or(|e| e != "osp" && e != "ospml")
            })
        })
    };
    let source_only = profile(&service, &fixture, json!({}), 2 * RATE_HZ).await;
    assert!(
        !native(&source_only),
        "without native frames every hotspot is source: {source_only}"
    );
    let with_native = profile(
        &service,
        &fixture,
        json!({ "include_native": true }),
        2 * RATE_HZ,
    )
    .await;
    assert!(
        native(&with_native),
        "the runtime's own frames appear on request: {with_native}"
    );
    service.shutdown().await;
}

/// [PROF-ERRORS]: a pid that names no process is `PROCESS_NOT_FOUND`; a live
/// process without an endpoint (this test binary) is `UNSUPPORTED_RUNTIME`.
#[tokio::test(flavor = "multi_thread")]
async fn a_pid_without_an_osprey_endpoint_is_classified() {
    let fixture = Fixture::start("lp-errors", Flavor::Default);
    let service = service(&fixture);
    let missing = json!({ "pid": 999_999 });
    assert_eq!(
        error_code(&service, methods::CPU_START, missing).await,
        -32001
    );
    let own = json!({ "pid": std::process::id() });
    assert_eq!(error_code(&service, methods::CPU_START, own).await, -32002);
    service.shutdown().await;
}

/// [PROF-CPU-START]: a pid with a session is `ALREADY_PROFILING`.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_session_on_the_same_program_is_refused() {
    let fixture = Fixture::start("lp-busy", Flavor::Default);
    let service = service(&fixture);
    let started = call(&service, methods::CPU_START, json!({ "pid": fixture.pid })).await;
    assert_eq!(
        error_code(&service, methods::CPU_START, json!({ "pid": fixture.pid })).await,
        -32004
    );
    drop(
        call(
            &service,
            methods::CPU_STOP,
            json!({ "session_id": started.get("session_id") }),
        )
        .await,
    );
    service.shutdown().await;
}

/// [PROF-CPU-SESSIONS]: a duration ends the session by itself.
#[tokio::test(flavor = "multi_thread")]
async fn a_duration_caps_the_recorded_time() {
    let fixture = Fixture::start("lp-duration", Flavor::Default);
    let service = service(&fixture);
    let started = call(
        &service,
        methods::CPU_START,
        json!({ "pid": fixture.pid, "sample_rate": RATE_HZ, "duration_secs": 1.0 }),
    )
    .await;
    let session = json!({ "session_id": started.get("session_id") });
    await_listing(
        &service,
        &session,
        |listed| listed.is_none(),
        "the session ending by itself",
    )
    .await;
    let report = call(&service, methods::CPU_STOP, session).await;
    let secs = report
        .get("duration_secs")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    assert!(
        (0.8..=1.5).contains(&secs),
        "recorded about the 1 s requested: {report}"
    );
    assert!(samples(&report) > 0, "{report}");
    service.shutdown().await;
}

/// [PROF-CPU-SNAPSHOT]: samples stream while recording, so a snapshot has them.
#[tokio::test(flavor = "multi_thread")]
async fn a_mid_session_snapshot_reports_the_samples_so_far() {
    let fixture = Fixture::start("lp-snapshot", Flavor::Default);
    let service = service(&fixture);
    let started = call(&service, methods::CPU_START, json!({ "pid": fixture.pid })).await;
    let session = json!({ "session_id": started.get("session_id") });
    await_samples(&service, &session, 1).await;
    let snapshot = call(&service, methods::CPU_SNAPSHOT, session.clone()).await;
    drop(call(&service, methods::CPU_STOP, session).await);
    assert!(
        samples(&snapshot) > 0,
        "once samples are streaming, the snapshot has them: {snapshot}"
    );
    service.shutdown().await;
}

/// [PROF-CPU-STOP]: a program killed mid-session keeps the samples taken while
/// it lived.
#[tokio::test(flavor = "multi_thread")]
async fn a_program_killed_mid_session_keeps_its_samples() {
    let mut fixture = Fixture::start("lp-killed", Flavor::Default);
    let service = service(&fixture);
    let started = call(&service, methods::CPU_START, json!({ "pid": fixture.pid })).await;
    let session = json!({ "session_id": started.get("session_id") });
    await_samples(&service, &session, 1).await;
    fixture.kill();
    let report = call(&service, methods::CPU_STOP, session).await;
    assert!(
        samples(&report) > 0,
        "samples taken before the kill are reported: {report}"
    );
    service.shutdown().await;
}

/// [PROF-CPU-PROGRESS] The live "hottest function" is the one the program
/// spends its time in, ranked by self samples. Ranked by inclusive samples, the
/// root of every whole stack, `main`, would always win.
#[tokio::test(flavor = "multi_thread")]
async fn live_progress_names_where_the_time_is_spent_not_the_entry_point() {
    let fixture = Fixture::start("lp-progress", Flavor::Default);
    let recorder = Arc::new(Recorder::default());
    let service = service_with(&fixture, Arc::<Recorder>::clone(&recorder));
    let session = start_session(&service, &fixture, json!({})).await;
    let enough = 3 * RATE_HZ;
    await_samples(&service, &session, enough).await;
    // Progress is a once-a-second tick that a loaded machine runs late: the
    // tops judged include one sent after all that evidence was in.
    let progress = || std::future::ready(recorder.progress_samples());
    let announced = poll("progress covering the samples", progress, |sent| {
        *sent >= enough
    })
    .await;
    let report = call(&service, methods::CPU_STOP, session).await;
    let tops = recorder.tops();
    let self_samples = |name: &str| {
        hot_functions(&report)
            .iter()
            .find(|hot| hot.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|hot| hot.get("self_samples").and_then(Value::as_i64))
            .unwrap_or(0)
    };
    assert!(!tops.is_empty(), "progress is sent while recording");
    assert!(
        samples(&report) >= announced,
        "the report holds every sample progress announced, {announced}: {report}"
    );
    assert!(
        !tops.iter().any(|top| top == "main"),
        "the entry point is never the hottest: {tops:?} {report}"
    );
    assert!(
        tops.iter().all(|top| self_samples(top) > 0),
        "every live top spends time of its own: {tops:?} {report}"
    );
    assert_eq!(
        tops.last().map(String::as_str),
        Some(HOT_FUNCTION),
        "the fixture's time is spent in spin: {report}"
    );
    service.shutdown().await;
}
