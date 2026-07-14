//! End-to-end pipeline tests: raw JSON on disk → [`crate::process_with`] →
//! four export files plus the terminal report, with a [`FixedSymbolizer`]
//! standing in for the external tools.

use crate::symbolize::{FixedSymbolizer, SymFrame};
use crate::testutil::temp_dir;
use crate::{process_profile, process_with, ProfileError, ProfileOptions};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Raw fixture: one image at base 4000 with slide 1000, a main fiber with 3
/// on-CPU samples and a waiting fiber with 2.
fn fixture_json() -> String {
    json!({
        "version": 1, "pid": 42, "rate_hz": 1000, "platform": "test",
        "start_unix_ns": 0, "end_unix_ns": 0, "dropped": 1, "exe": "/bin/app",
        "images": [{"path": "/bin/app", "base": 4000, "slide": 1000}],
        "threads": [{"fiber": 0, "label": "main"}, {"fiber": 2, "label": "fiber"}],
        "stacks": [[5100, 5201], [5300]],
        "samples": [
            [0, 0, 0, 0], [1_000_000, 0, 0, 0], [2_000_000, 0, 1, 0],
            [0, 1, 1, 1], [1_000_000, 1, 1, 1]
        ],
    })
    .to_string()
}

/// Unslid fixture addresses: leaf 5100 → 4100; return 5201 → 5201−1−1000 =
/// 4200; leaf 5300 → 4300.
fn fixture_symbolizer() -> FixedSymbolizer {
    let mut sym = FixedSymbolizer::default();
    let _ = sym
        .map
        .insert(4100, vec![SymFrame::new("fib", "/abs/fib.osp", 5)]);
    let _ = sym
        .map
        .insert(4200, vec![SymFrame::new("main", "/abs/fib.osp", 2)]);
    let _ = sym.map.insert(
        4300,
        vec![SymFrame::new("fiber_thread_func", "/rt/fiber.c", 33)],
    );
    sym
}

fn options(dir: &Path) -> ProfileOptions {
    let raw_path = dir.join("raw.osprof.json");
    std::fs::write(&raw_path, fixture_json()).unwrap();
    ProfileOptions {
        raw_path,
        binary_path: PathBuf::from("/bin/app"),
        source_path: "/abs/fib.osp".to_owned(),
        out_dir: dir.join("out"),
        stem: "fib".to_owned(),
        color: false,
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn pipeline_writes_all_four_exports_and_reports() {
    let dir = temp_dir("e2e");
    let opts = options(&dir);
    let outcome = process_with(&opts, &fixture_symbolizer()).unwrap();

    assert_eq!(
        outcome.speedscope_path,
        opts.out_dir.join("fib.speedscope.json")
    );
    assert_eq!(outcome.cpuprofile_path, opts.out_dir.join("fib.cpuprofile"));
    assert_eq!(outcome.folded_path, opts.out_dir.join("fib.folded"));
    assert_eq!(outcome.summary_path, opts.out_dir.join("fib.profile.json"));

    let speedscope = read_json(&outcome.speedscope_path);
    assert_eq!(
        speedscope
            .pointer("/profiles")
            .and_then(Value::as_array)
            .unwrap()
            .len(),
        2
    );
    let frame_names: Vec<&str> = speedscope
        .pointer("/shared/frames")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(|f| f.get("name").and_then(Value::as_str))
        .collect();
    assert!(frame_names.contains(&"fib") && frame_names.contains(&"fiber_thread_func"));

    let cpuprofile = read_json(&outcome.cpuprofile_path);
    assert!(
        cpuprofile
            .get("nodes")
            .and_then(Value::as_array)
            .unwrap()
            .len()
            >= 3
    );
    assert_eq!(
        cpuprofile
            .get("samples")
            .and_then(Value::as_array)
            .unwrap()
            .len(),
        3
    );

    let folded = std::fs::read_to_string(&outcome.folded_path).unwrap();
    let lines: Vec<&str> = folded.lines().collect();
    assert_eq!(
        lines,
        [
            "fiber-2;fiber_thread_func 2",
            "main;fiber_thread_func 1",
            "main;main;fib 2"
        ]
    );

    let summary = read_json(&outcome.summary_path);
    assert_eq!(
        summary.get("program").and_then(Value::as_str),
        Some("/abs/fib.osp")
    );
    assert_eq!(
        summary.get("droppedSamples").and_then(Value::as_u64),
        Some(1)
    );
    let hot = summary.pointer("/hotFunctions/0").unwrap();
    assert_eq!(hot.get("name").and_then(Value::as_str), Some("fib"));
    assert_eq!(hot.get("kind").and_then(Value::as_str), Some("user"));

    assert!(
        outcome.report.contains("fib  fib.osp:5"),
        "{}",
        outcome.report
    );
    assert!(outcome
        .report
        .contains("fibers: main 100% on-cpu · fiber-2 waiting"));
    assert!(outcome.report.contains("note: only 3 on-CPU samples"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn process_profile_uses_the_real_symbolizer_without_failing() {
    let dir = temp_dir("e2e-real");
    let exe = std::env::current_exe().unwrap();
    let mut opts = options(&dir);
    opts.binary_path.clone_from(&exe);
    // Point the image at the test binary so the real tool has a valid object;
    // the tiny addresses resolve to nothing and must fall back to hex names.
    let raw = fixture_json().replace("/bin/app", &exe.to_string_lossy());
    std::fs::write(&opts.raw_path, raw).unwrap();
    let outcome = process_profile(&opts).unwrap();
    assert!(outcome.report.contains("samples @ 1000Hz"));
    assert!(outcome.speedscope_path.is_file() && outcome.summary_path.is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_raw_file_is_an_io_error_with_path_context() {
    let dir = temp_dir("e2e-missing");
    let mut opts = options(&dir);
    opts.raw_path = dir.join("nope.json");
    let err = process_with(&opts, &fixture_symbolizer()).unwrap_err();
    assert!(matches!(err, ProfileError::Io { .. }));
    assert!(err.to_string().contains("nope.json"));
    assert!(std::error::Error::source(&err).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unwritable_out_dir_is_an_io_error() {
    let dir = temp_dir("e2e-outdir");
    let mut opts = options(&dir);
    opts.out_dir = dir.join("blocked");
    std::fs::write(&opts.out_dir, b"a file, not a directory").unwrap();
    let err = process_with(&opts, &fixture_symbolizer()).unwrap_err();
    assert!(matches!(err, ProfileError::Io { .. }), "{err}");
    assert!(err.to_string().contains("blocked"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn error_display_and_sources_cover_every_variant() {
    let parse = process_with(
        &ProfileOptions {
            raw_path: {
                let dir = temp_dir("e2e-parse");
                let path = dir.join("bad.json");
                std::fs::write(&path, "not json").unwrap();
                path
            },
            binary_path: PathBuf::from("/bin/app"),
            source_path: "x.osp".to_owned(),
            out_dir: PathBuf::from("."),
            stem: "x".to_owned(),
            color: false,
        },
        &fixture_symbolizer(),
    )
    .unwrap_err();
    assert!(parse.to_string().contains("invalid raw profile JSON in"));
    assert!(std::error::Error::source(&parse).is_some());

    let invalid = ProfileError::Invalid("stack 1 is empty".to_owned());
    assert_eq!(invalid.to_string(), "invalid raw profile: stack 1 is empty");
    assert!(std::error::Error::source(&invalid).is_none());

    let symbolize = ProfileError::Symbolize("tool crashed".to_owned());
    assert_eq!(symbolize.to_string(), "symbolization failed: tool crashed");
    assert!(std::error::Error::source(&symbolize).is_none());
}
