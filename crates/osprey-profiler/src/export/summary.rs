//! Editor-integration summary export [PROF-CLI-RUN]: totals, the per-fiber
//! state split, hot functions (self/total), and hot lines — the data source
//! for the VS Code heat decorations [PROF-VSCODE-HEAT].

use crate::model::{pct, Model};
use serde_json::{json, Value};

/// Hot-function cap in the summary.
const MAX_HOT_FUNCTIONS: usize = 50;
/// Hot-line cap in the summary.
const MAX_HOT_LINES: usize = 100;
/// Lines below this share of on-CPU samples are noise, not heat.
const MIN_LINE_PCT: f64 = 0.5;

/// Build the `<stem>.profile.json` document (camelCase keys, version 1).
pub(crate) fn summary_json(program: &str, model: &Model) -> Value {
    json!({
        "version": 1,
        "program": program,
        "wallSeconds": round3(model.wall_seconds),
        "cpuSeconds": round3(model.cpu_seconds),
        "sampleCount": model.sample_count,
        "rateHz": model.rate_hz,
        "droppedSamples": model.dropped,
        "fibers": fibers_json(model),
        "hotFunctions": hot_functions(model),
        "hotLines": hot_lines(model),
    })
}

/// Per-fiber totals from ALL samples.
fn fibers_json(model: &Model) -> Vec<Value> {
    model
        .fibers
        .iter()
        .map(|fiber| {
            json!({
                "id": fiber.id,
                "label": fiber.label,
                "samples": fiber.samples,
                "oncpuSamples": fiber.oncpu_samples,
            })
        })
        .collect()
}

/// Top functions by self share of on-CPU samples (already sorted).
fn hot_functions(model: &Model) -> Vec<Value> {
    model
        .funcs
        .iter()
        .take(MAX_HOT_FUNCTIONS)
        .map(|func| {
            json!({
                "name": func.name,
                "file": func.file,
                "line": func.line,
                "selfPct": round1(pct(func.self_samples, model.oncpu_samples)),
                "totalPct": round1(pct(func.total_samples, model.oncpu_samples)),
                "selfSamples": func.self_samples,
                "totalSamples": func.total_samples,
                "kind": func.kind.as_str(),
            })
        })
        .collect()
}

/// Hot lines at or above [`MIN_LINE_PCT`] of on-CPU samples.
fn hot_lines(model: &Model) -> Vec<Value> {
    model
        .lines
        .iter()
        .filter(|line| pct(line.samples, model.oncpu_samples) >= MIN_LINE_PCT)
        .take(MAX_HOT_LINES)
        .map(|line| {
            json!({
                "file": line.file,
                "line": line.line,
                "pct": round1(pct(line.samples, model.oncpu_samples)),
                "samples": line.samples,
            })
        })
        .collect()
}

/// One-decimal rounding for percentages.
fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Millisecond-precision rounding for second totals.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::build_model;
    use crate::raw::{Profile, Sample, Thread};
    use crate::symbolize::SymFrame;
    use crate::testutil::{model_of, osp_frame, sample, thread};

    fn fixture() -> Value {
        let (_, model) = model_of(
            vec![thread(0, "main"), thread(4, "fiber")],
            vec![vec![100, 200], vec![300]],
            vec![
                sample(0, 0, 0, true),
                sample(1_000_000_000, 0, 0, true),
                sample(2_000_000_000, 0, 1, true),
                sample(3_000_000_000, 1, 1, false),
            ],
            &[
                vec![osp_frame("fib", 5), osp_frame("main", 2)],
                vec![SymFrame::new("memcpy", "/rt/string.c", 40)],
            ],
        );
        summary_json("/abs/fib.osp", &model)
    }

    #[test]
    fn header_totals_and_program() {
        let doc = fixture();
        assert_eq!(doc.get("version").and_then(Value::as_u64), Some(1));
        assert_eq!(
            doc.get("program").and_then(Value::as_str),
            Some("/abs/fib.osp")
        );
        assert_eq!(doc.get("wallSeconds").and_then(Value::as_f64), Some(3.0));
        assert_eq!(doc.get("cpuSeconds").and_then(Value::as_f64), Some(0.003));
        assert_eq!(doc.get("sampleCount").and_then(Value::as_u64), Some(4));
        assert_eq!(doc.get("rateHz").and_then(Value::as_u64), Some(1000));
        assert_eq!(doc.get("droppedSamples").and_then(Value::as_u64), Some(0));
    }

    #[test]
    fn fibers_report_the_state_split_from_all_samples() {
        let doc = fixture();
        let fibers = doc.get("fibers").and_then(Value::as_array).unwrap();
        assert_eq!(fibers.len(), 2);
        let main = fibers.first().unwrap();
        assert_eq!(main.get("id").and_then(Value::as_u64), Some(0));
        assert_eq!(main.get("label").and_then(Value::as_str), Some("main"));
        assert_eq!(main.get("samples").and_then(Value::as_u64), Some(3));
        assert_eq!(main.get("oncpuSamples").and_then(Value::as_u64), Some(3));
        let fiber = fibers.get(1).unwrap();
        assert_eq!(fiber.get("label").and_then(Value::as_str), Some("fiber-4"));
        assert_eq!(fiber.get("oncpuSamples").and_then(Value::as_u64), Some(0));
    }

    #[test]
    fn hot_functions_carry_pcts_kind_and_sort_order() {
        let doc = fixture();
        let funcs = doc.get("hotFunctions").and_then(Value::as_array).unwrap();
        let fib = funcs.first().unwrap();
        assert_eq!(fib.get("name").and_then(Value::as_str), Some("fib"));
        assert_eq!(
            fib.get("file").and_then(Value::as_str),
            Some("/src/app.osp")
        );
        assert_eq!(fib.get("line").and_then(Value::as_u64), Some(5));
        assert_eq!(fib.get("selfPct").and_then(Value::as_f64), Some(66.7));
        assert_eq!(fib.get("totalPct").and_then(Value::as_f64), Some(66.7));
        assert_eq!(fib.get("selfSamples").and_then(Value::as_u64), Some(2));
        assert_eq!(fib.get("kind").and_then(Value::as_str), Some("user"));
        let memcpy = funcs
            .iter()
            .find(|f| f.get("name").and_then(Value::as_str) == Some("memcpy"));
        assert_eq!(
            memcpy.unwrap().get("kind").and_then(Value::as_str),
            Some("runtime")
        );
    }

    #[test]
    fn hot_lines_keep_only_significant_osp_lines() {
        let doc = fixture();
        let lines = doc.get("hotLines").and_then(Value::as_array).unwrap();
        assert_eq!(lines.len(), 1);
        let hot = lines.first().unwrap();
        assert_eq!(
            hot.get("file").and_then(Value::as_str),
            Some("/src/app.osp")
        );
        assert_eq!(hot.get("line").and_then(Value::as_u64), Some(5));
        assert_eq!(hot.get("pct").and_then(Value::as_f64), Some(66.7));
        assert_eq!(hot.get("samples").and_then(Value::as_u64), Some(2));
    }

    /// 55 single-frame functions: the function list caps at 50 and sub-0.5%
    /// lines drop out entirely once a dominant line dwarfs them.
    #[test]
    fn caps_and_noise_filtering() {
        let count = 55;
        let stacks: Vec<Vec<u64>> = (0..count).map(|i| vec![1000 + i]).collect();
        let mut samples: Vec<Sample> = (0..count)
            .map(|i| sample(i, 0, usize::try_from(i).unwrap(), true))
            .collect();
        // A dominant stack takes 300 extra samples -> each 1-sample line is
        // 1/355 = 0.28% < 0.5%.
        samples.extend((0..300).map(|i| sample(1_000_000 + i, 0, 0, true)));
        let prof = Profile {
            rate_hz: 1000,
            dropped: 0,
            images: vec![],
            threads: vec![Thread {
                fiber: 0,
                label: "main".to_owned(),
            }],
            stacks,
            samples,
        };
        let sym_stacks: Vec<Vec<SymFrame>> = (0..count)
            .map(|i| vec![osp_frame(&format!("f{i}"), u32::try_from(i).unwrap() + 1)])
            .collect();
        let doc = summary_json("x.osp", &build_model(&prof, &sym_stacks));
        assert_eq!(
            doc.get("hotFunctions")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            50
        );
        let lines = doc.get("hotLines").and_then(Value::as_array).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines.first().unwrap().get("line").and_then(Value::as_u64),
            Some(1)
        );
    }
}
