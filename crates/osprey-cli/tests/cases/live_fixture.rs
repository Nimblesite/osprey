//! The live-attach fixture both live-profiler suites run [PROF-ATTACH-ENDPOINT]:
//! compiled once with debug info by this build's `osprey`, then started per
//! test with a private `TMPDIR`, so its endpoint lives where only that test
//! looks.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The fixture's hot function.
pub(crate) const HOT_FUNCTION: &str = "spin";
/// The sample rate every session asks for: `n * RATE_HZ` samples are what `n`
/// seconds record on an idle machine.
pub(crate) const RATE_HZ: i64 = 997;
/// How long a starved sampler has to record what a test needs before it fails.
/// Tests wait for samples, never for seconds: a loaded machine samples slower.
pub(crate) const RECORDING_DEADLINE: Duration = Duration::from_secs(120);
/// How often a test re-reads `cpu.list` while it waits.
pub(crate) const POLL: Duration = Duration::from_millis(50);
/// How long a test waits for the fixture's endpoint to appear.
const STARTUP: Duration = Duration::from_secs(20);

/// Which twin of the fixture runs: the Default `live_attach.osp` or its ML
/// twin `live_attach.ospml`, the same program in each flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flavor {
    Default,
    Ml,
}

impl Flavor {
    /// The fixture's source file name.
    pub(crate) const fn source(self) -> &'static str {
        match self {
            Self::Default => "live_attach.osp",
            Self::Ml => "live_attach.ospml",
        }
    }

    /// `spin` in this twin: its signature through its last arm.
    pub(crate) const fn spin_lines(self) -> std::ops::RangeInclusive<i64> {
        match self {
            Self::Default => 9..=13,
            Self::Ml => 9..=12,
        }
    }

    const fn exe(self) -> &'static str {
        match self {
            Self::Default => "live_attach",
            Self::Ml => "live_attach_ml",
        }
    }
}

/// The fixture binary for `flavor`, compiled once for every test in this
/// file. Under target/, so a run leaves nothing in the system temp directory.
pub(crate) fn fixture_exe(flavor: Flavor) -> &'static Path {
    static DEFAULT: OnceLock<PathBuf> = OnceLock::new();
    static ML: OnceLock<PathBuf> = OnceLock::new();
    let compiled = match flavor {
        Flavor::Default => &DEFAULT,
        Flavor::Ml => &ML,
    };
    compiled.get_or_init(|| {
        let exe = Path::new(env!("CARGO_TARGET_TMPDIR")).join(flavor.exe());
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(flavor.source());
        let status = Command::new(env!("CARGO_BIN_EXE_osprey"))
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .arg(&source)
            .args(["--compile", "--profile", "--quiet", "-o"])
            .arg(&exe)
            .status()
            .expect("osprey runs");
        assert!(status.success(), "the fixture compiles with debug info");
        exe
    })
}

/// The running fixture with its own `TMPDIR`, so its endpoint lives in a
/// directory only this test reads. Killed, and its directory removed, on drop.
pub(crate) struct Fixture {
    child: Child,
    pub(crate) pid: u32,
    pub(crate) tmp: PathBuf,
    pub(crate) socket_dir: PathBuf,
}

impl Fixture {
    /// The `flavor` twin, running.
    pub(crate) fn start(name: &str, flavor: Flavor) -> Self {
        // A Unix socket path is at most 103 bytes on macOS, and a coverage
        // build's target directory is already most of that; the system temp
        // directory leaves room for `osprey-diagnostics-<uid>/<pid>.sock`.
        let tmp = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&tmp));
        std::fs::create_dir_all(&tmp).expect("scratch directory");
        let child = Command::new(fixture_exe(flavor))
            .env("TMPDIR", &tmp)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the fixture starts");
        let pid = child.id();
        let socket_dir = wait_for_endpoint(&tmp, pid);
        Self {
            child,
            pid,
            tmp,
            socket_dir,
        }
    }

    pub(crate) fn kill(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.kill();
        drop(std::fs::remove_dir_all(&self.tmp));
    }
}

/// The private endpoint directory once the profiler recognises a ready process.
/// Binding creates the socket before listening; its filename is not readiness.
fn wait_for_endpoint(tmp: &Path, pid: u32) -> PathBuf {
    let deadline = Instant::now() + STARTUP;
    while Instant::now() < deadline {
        let found = std::fs::read_dir(tmp)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|dir| dir.join(format!("{pid}.sock")).exists())
            .find(|dir| endpoint_ready(dir, pid));
        if let Some(dir) = found {
            return dir;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no endpoint for PID {pid} under {}", tmp.display());
}

fn endpoint_ready(dir: &Path, pid: u32) -> bool {
    let runtime = Arc::new(lsp_profiler_osprey::OspreyRuntime::new(
        lsp_profiler_osprey::OspreyConfig {
            socket_dir: Some(dir.to_path_buf()),
        },
    ));
    lsp_profiler::process::recognise(&[runtime], pid).is_some_and(|process| process.info.attachable)
}

/// `session`'s entry in a `cpu.list` result: `None` once it has ended.
pub(crate) fn listed_session<'a>(list: &'a Value, session: &Value) -> Option<&'a Value> {
    list.get("sessions")
        .and_then(Value::as_array)
        .and_then(|all| {
            all.iter()
                .find(|s| s.get("session_id") == session.get("session_id"))
        })
}

/// A listed session's live sample count; 0 once it is not listed.
pub(crate) fn listed_samples(listed: Option<&Value>) -> i64 {
    listed
        .and_then(|s| s.get("sample_count"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}
