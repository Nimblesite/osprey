//! Raw profile parsing and validation. Implements [PROF-RAW-FORMAT]: the
//! runtime writes one JSON document at exit with leaf-first raw stacks and
//! `[t_rel_ns, thread_index, stack_index, state]` sample rows; this module
//! turns that document into an index-checked [`Profile`].

use crate::ProfileError;
use serde::Deserialize;
use std::path::Path;

/// The only raw-format version this crate understands.
const SUPPORTED_VERSION: u64 = 1;
/// Sample `state` value for a thread that was running on a CPU.
const STATE_ON_CPU: u64 = 0;
/// Sample `state` value for a thread that was blocked/waiting.
const STATE_WAITING: u64 = 1;

/// One loaded binary image: raw pcs are mapped to the image with the greatest
/// `base <= pc`, then unslid by `slide` [PROF-SYMBOLIZE-OFFLINE].
#[derive(Debug, Deserialize)]
pub(crate) struct Image {
    /// Filesystem path of the image (executable or dylib).
    pub path: String,
    /// Load address of the image in the profiled process.
    pub base: u64,
    /// ASLR slide: `pc - slide` is the address in the on-disk image.
    #[serde(default)]
    pub slide: u64,
}

/// One registered thread [PROF-COLLECT-REGISTRY]. Osprey fibers are 1:1
/// pthreads, so a thread row is a fiber.
#[derive(Debug, Deserialize)]
pub(crate) struct Thread {
    /// The fiber id the thread registered with (0 = main; effect
    /// continuation threads register with -1 [PROF-COLLECT-REGISTRY]).
    pub fiber: i64,
    /// The registration label: `main`, `fiber`, or `effect`.
    pub label: String,
}

/// One validated sample: indices are checked against `threads`/`stacks`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sample {
    /// Nanoseconds since the first possible sample (relative time).
    pub t_ns: u64,
    /// Index into [`Profile::threads`].
    pub thread: usize,
    /// Index into [`Profile::stacks`].
    pub stack: usize,
    /// `true` when the thread was on-CPU, `false` when waiting.
    pub on_cpu: bool,
}

/// A validated raw profile: every sample's indices are in range, every stack
/// is non-empty (frame 0 is the precise interrupted pc), and `rate_hz > 0`.
#[derive(Debug)]
pub(crate) struct Profile {
    /// Nominal sampling rate in Hz.
    pub rate_hz: u64,
    /// Samples the runtime dropped (ring overflow).
    pub dropped: u64,
    /// Loaded images, sorted by ascending `base`.
    pub images: Vec<Image>,
    /// Registered threads (fibers).
    pub threads: Vec<Thread>,
    /// Leaf-first raw address stacks; frames after index 0 are RETURN
    /// addresses.
    pub stacks: Vec<Vec<u64>>,
    /// Validated samples in file order.
    pub samples: Vec<Sample>,
}

/// The serde mirror of the on-disk document; unknown fields (pid, exe,
/// platform, timestamps) are ignored.
#[derive(Debug, Deserialize)]
struct RawProfile {
    version: u64,
    rate_hz: u64,
    #[serde(default)]
    dropped: u64,
    images: Vec<Image>,
    threads: Vec<Thread>,
    stacks: Vec<Vec<u64>>,
    samples: Vec<(u64, u64, u64, u64)>,
}

/// Read and validate the raw profile at `path`.
pub(crate) fn parse_file(path: &Path) -> Result<Profile, ProfileError> {
    let text = std::fs::read_to_string(path).map_err(|source| ProfileError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_str(&text, path)
}

/// Parse and validate raw-profile JSON; `path` is only used for error context.
pub(crate) fn parse_str(text: &str, path: &Path) -> Result<Profile, ProfileError> {
    let raw: RawProfile = serde_json::from_str(text).map_err(|source| ProfileError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    validate(raw)
}

/// Enforce every [PROF-RAW-FORMAT] invariant, producing the typed [`Profile`].
fn validate(raw: RawProfile) -> Result<Profile, ProfileError> {
    check_version(raw.version)?;
    check_rate(raw.rate_hz)?;
    check_stacks(&raw.stacks)?;
    let samples = raw
        .samples
        .iter()
        .map(|row| check_sample(*row, raw.threads.len(), raw.stacks.len()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut images = raw.images;
    images.sort_by_key(|image| image.base);
    Ok(Profile {
        rate_hz: raw.rate_hz,
        dropped: raw.dropped,
        images,
        threads: raw.threads,
        stacks: raw.stacks,
        samples,
    })
}

/// Only `version: 1` is accepted; anything else is a different contract.
fn check_version(version: u64) -> Result<(), ProfileError> {
    if version == SUPPORTED_VERSION {
        return Ok(());
    }
    Err(ProfileError::Invalid(format!(
        "unsupported raw profile version {version} (expected {SUPPORTED_VERSION})"
    )))
}

/// The sampling rate divides sample counts into seconds — zero is malformed.
fn check_rate(rate_hz: u64) -> Result<(), ProfileError> {
    if rate_hz > 0 {
        return Ok(());
    }
    Err(ProfileError::Invalid("rate_hz must be positive".to_owned()))
}

/// Every stack must carry at least the interrupted pc (frame 0).
fn check_stacks(stacks: &[Vec<u64>]) -> Result<(), ProfileError> {
    stacks
        .iter()
        .position(Vec::is_empty)
        .map_or(Ok(()), |index| {
            Err(ProfileError::Invalid(format!("stack {index} is empty")))
        })
}

/// Validate one `[t_rel_ns, thread_index, stack_index, state]` row.
fn check_sample(
    row: (u64, u64, u64, u64),
    threads: usize,
    stacks: usize,
) -> Result<Sample, ProfileError> {
    let (t_ns, thread_raw, stack_raw, state) = row;
    let thread = index_in(thread_raw, threads, "thread")?;
    let stack = index_in(stack_raw, stacks, "stack")?;
    let on_cpu = match state {
        STATE_ON_CPU => true,
        STATE_WAITING => false,
        other => {
            return Err(ProfileError::Invalid(format!(
                "sample state must be {STATE_ON_CPU} or {STATE_WAITING}, got {other}"
            )))
        }
    };
    Ok(Sample {
        t_ns,
        thread,
        stack,
        on_cpu,
    })
}

/// Bounds-check a raw index against the table it points into.
fn index_in(value: u64, len: usize, what: &str) -> Result<usize, ProfileError> {
    match usize::try_from(value) {
        Ok(index) if index < len => Ok(index),
        _ => Err(ProfileError::Invalid(format!(
            "{what} index {value} out of range (0..{len})"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The [PROF-RAW-FORMAT] example document, extra fields included.
    const EXAMPLE: &str = r#"{"version":1,"pid":123,"rate_hz":997,"platform":"macos-arm64",
        "start_unix_ns":1,"end_unix_ns":2,"dropped":3,"exe":"/tmp/x.out",
        "images":[{"path":"/usr/lib/dyld","base":6000000000},
                  {"path":"/tmp/x.out","base":4295000000,"slide":12345}],
        "threads":[{"fiber":0,"label":"main"},{"fiber":1,"label":"fiber"}],
        "stacks":[[4301231,4301100],[4301500]],
        "samples":[[12345,0,0,0],[1015000,1,1,1]]}"#;

    fn parse(text: &str) -> Result<Profile, ProfileError> {
        parse_str(text, Path::new("/tmp/raw.json"))
    }

    #[test]
    fn parses_the_spec_example_and_ignores_unknown_fields() {
        let profile = parse(EXAMPLE).unwrap();
        assert_eq!(profile.rate_hz, 997);
        assert_eq!(profile.dropped, 3);
        assert_eq!(profile.threads.len(), 2);
        assert_eq!(profile.stacks.len(), 2);
        assert_eq!(profile.samples.len(), 2);
        let first = profile.samples.first().unwrap();
        assert!(first.on_cpu);
        assert_eq!((first.t_ns, first.thread, first.stack), (12345, 0, 0));
        let second = profile.samples.get(1).unwrap();
        assert!(!second.on_cpu);
    }

    #[test]
    fn sorts_images_by_base_and_defaults_slide() {
        let profile = parse(EXAMPLE).unwrap();
        let first = profile.images.first().unwrap();
        assert_eq!((first.base, first.slide), (4_295_000_000, 12345));
        assert_eq!(profile.images.get(1).unwrap().slide, 0);
    }

    #[test]
    fn parses_negative_effect_fiber_ids() {
        let text = EXAMPLE.replacen(
            r#"{"fiber":1,"label":"fiber"}"#,
            r#"{"fiber":-1,"label":"effect"}"#,
            1,
        );
        let profile = parse(&text).unwrap();
        let effect = profile.threads.get(1).unwrap();
        assert_eq!((effect.fiber, effect.label.as_str()), (-1, "effect"));
    }

    #[test]
    fn rejects_wrong_version() {
        let text = EXAMPLE.replacen("\"version\":1", "\"version\":2", 1);
        let err = parse(&text).unwrap_err();
        assert!(err.to_string().contains("version 2"), "got: {err}");
    }

    #[test]
    fn rejects_zero_rate() {
        let text = EXAMPLE.replacen("\"rate_hz\":997", "\"rate_hz\":0", 1);
        assert!(parse(&text).unwrap_err().to_string().contains("rate_hz"));
    }

    #[test]
    fn rejects_out_of_range_thread_index() {
        let text = EXAMPLE.replacen("[12345,0,0,0]", "[12345,9,0,0]", 1);
        assert!(parse(&text)
            .unwrap_err()
            .to_string()
            .contains("thread index 9"));
    }

    #[test]
    fn rejects_out_of_range_stack_index() {
        let text = EXAMPLE.replacen("[12345,0,0,0]", "[12345,0,7,0]", 1);
        assert!(parse(&text)
            .unwrap_err()
            .to_string()
            .contains("stack index 7"));
    }

    #[test]
    fn rejects_unknown_sample_state() {
        let text = EXAMPLE.replacen("[12345,0,0,0]", "[12345,0,0,4]", 1);
        assert!(parse(&text).unwrap_err().to_string().contains("state"));
    }

    #[test]
    fn rejects_empty_stack() {
        let text = EXAMPLE.replacen("[4301500]", "[]", 1);
        assert!(parse(&text)
            .unwrap_err()
            .to_string()
            .contains("stack 1 is empty"));
    }

    #[test]
    fn rejects_malformed_sample_row_arity() {
        let text = EXAMPLE.replacen("[12345,0,0,0]", "[12345,0,0]", 1);
        assert!(matches!(
            parse(&text).unwrap_err(),
            ProfileError::Parse { .. }
        ));
    }

    #[test]
    fn rejects_non_json_with_parse_error_context() {
        let err = parse("not json").unwrap_err();
        assert!(err.to_string().contains("/tmp/raw.json"), "got: {err}");
    }

    #[test]
    fn parse_file_reads_from_disk_and_reports_missing_files() {
        let dir = crate::testutil::temp_dir("raw");
        let path = dir.join("profile.json");
        std::fs::write(&path, EXAMPLE).unwrap();
        assert_eq!(parse_file(&path).unwrap().rate_hz, 997);
        let missing = parse_file(&dir.join("missing.json")).unwrap_err();
        assert!(matches!(missing, ProfileError::Io { .. }));
        assert!(missing.to_string().contains("missing.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
