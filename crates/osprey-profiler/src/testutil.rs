//! Shared test fixtures: compact builders for validated profiles, symbolized
//! frames, and unique scratch directories.

use crate::model::{build_model, Model};
use crate::raw::{Image, Profile, Sample, Thread};
use crate::symbolize::SymFrame;
use std::path::PathBuf;

/// Sampling rate every fixture profile uses (1 sample = 1 ms).
const TEST_RATE_HZ: u64 = 1000;

/// A thread row.
pub(crate) fn thread(fiber: i64, label: &str) -> Thread {
    Thread {
        fiber,
        label: label.to_owned(),
    }
}

/// A validated sample.
pub(crate) fn sample(t_ns: u64, thread: usize, stack: usize, on_cpu: bool) -> Sample {
    Sample {
        t_ns,
        thread,
        stack,
        on_cpu,
    }
}

/// A validated profile with one zero-slide image at base 0.
pub(crate) fn profile(
    threads: Vec<Thread>,
    stacks: Vec<Vec<u64>>,
    samples: Vec<Sample>,
) -> Profile {
    Profile {
        rate_hz: TEST_RATE_HZ,
        dropped: 0,
        images: vec![Image {
            path: "/bin/app".to_owned(),
            base: 0,
            slide: 0,
            text: 0,
            text_size: 0,
        }],
        threads,
        stacks,
        samples,
    }
}

/// A fixture profile together with its aggregated model — the shared
/// scaffolding of every exporter/report test.
pub(crate) fn model_of(
    threads: Vec<Thread>,
    stacks: Vec<Vec<u64>>,
    samples: Vec<Sample>,
    sym_stacks: &[Vec<SymFrame>],
) -> (Profile, Model) {
    let prof = profile(threads, stacks, samples);
    let model = build_model(&prof, sym_stacks);
    (prof, model)
}

/// A user frame in the canonical fixture source file.
pub(crate) fn osp_frame(name: &str, line: u32) -> SymFrame {
    SymFrame::new(name, "/src/app.osp", line)
}

/// Assert a JSON object field holds an expected scalar. The exporter tests
/// check dozens of fields each; every one of them was spelling the same
/// `get(..).and_then(Value::as_..)` accessor pair inline, which buried the
/// expectation in accessor noise and reported failures without naming the
/// field. One row per field, and the key is in the failure message.
pub(crate) fn field_str(doc: &serde_json::Value, key: &str, want: &str) {
    assert_eq!(
        doc.get(key).and_then(serde_json::Value::as_str),
        Some(want),
        "field `{key}`"
    );
}

/// [`field_str`] for an unsigned-integer field.
pub(crate) fn field_u64(doc: &serde_json::Value, key: &str, want: u64) {
    assert_eq!(
        doc.get(key).and_then(serde_json::Value::as_u64),
        Some(want),
        "field `{key}`"
    );
}

/// [`field_str`] for a floating-point field.
pub(crate) fn field_f64(doc: &serde_json::Value, key: &str, want: f64) {
    assert_eq!(
        doc.get(key).and_then(serde_json::Value::as_f64),
        Some(want),
        "field `{key}`"
    );
}

/// A unique, created scratch directory for one test.
pub(crate) fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "osprey-profiler-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
