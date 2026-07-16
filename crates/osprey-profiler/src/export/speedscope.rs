//! Speedscope export [PROF-CLI-RUN]: one `"sampled"` profile per fiber over
//! a shared interned frame table. ALL samples are included — waiting stacks
//! show their blocking call naturally, making this the wall-clock view.

use crate::model::{ns_to_secs, u64_to_f64, Model};
use crate::raw::{Profile, Sample};
use serde_json::{json, Value};

/// The speedscope file-format schema URL.
const SCHEMA: &str = "https://www.speedscope.app/file-format-schema.json";

/// Build the complete speedscope document.
pub(crate) fn speedscope_json(profile: &Profile, model: &Model) -> Value {
    json!({
        "$schema": SCHEMA,
        "exporter": "osprey",
        "shared": { "frames": frames_json(model) },
        "profiles": profiles_json(profile, model),
    })
}

/// The shared interned frame table (no duplicates by construction).
fn frames_json(model: &Model) -> Vec<Value> {
    model
        .frames
        .iter()
        .map(|f| json!({ "name": f.name, "file": f.file, "line": f.line }))
        .collect()
}

/// One sampled profile per thread row that actually has samples.
fn profiles_json(profile: &Profile, model: &Model) -> Vec<Value> {
    per_thread_samples(profile)
        .into_iter()
        .filter(|(_, samples)| !samples.is_empty())
        .map(|(thread, samples)| thread_profile(model, thread, &samples, profile.rate_hz))
        .collect()
}

/// Group samples by thread, preserving file (time) order within each.
fn per_thread_samples(profile: &Profile) -> Vec<(usize, Vec<Sample>)> {
    let mut per: Vec<Vec<Sample>> = vec![Vec::new(); profile.threads.len()];
    for sample in &profile.samples {
        if let Some(bucket) = per.get_mut(sample.thread) {
            bucket.push(*sample);
        }
    }
    per.into_iter().enumerate().collect()
}

/// One fiber's `"sampled"` profile: root-first stacks with per-sample weights
/// in seconds (delta to the previous sample; the first weighs `1/rate`).
fn thread_profile(model: &Model, thread: usize, samples: &[Sample], rate_hz: u64) -> Value {
    let weights = sample_weights(samples, rate_hz);
    let end: f64 = weights.iter().sum();
    let stacks: Vec<Vec<usize>> = samples.iter().map(|s| model.root_first(s.stack)).collect();
    let name = model
        .fibers
        .get(thread)
        .map_or_else(|| format!("thread-{thread}"), |fiber| fiber.label.clone());
    json!({
        "type": "sampled",
        "name": name,
        "unit": "seconds",
        "startValue": 0,
        "endValue": end,
        "samples": stacks,
        "weights": weights,
    })
}

/// Per-sample weights: `samples.len() == weights.len()` always holds.
fn sample_weights(samples: &[Sample], rate_hz: u64) -> Vec<f64> {
    let first = if rate_hz == 0 {
        0.0
    } else {
        1.0 / u64_to_f64(rate_hz)
    };
    samples
        .iter()
        .scan(None, |prev: &mut Option<u64>, sample| {
            let weight = prev.map_or(first, |p| ns_to_secs(sample.t_ns.saturating_sub(p)));
            *prev = Some(sample.t_ns);
            Some(weight)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{model_of, osp_frame, sample, thread};

    fn fixture() -> Value {
        // Threads: main (3 samples incl. one waiting), fiber-7 (0 samples).
        let (prof, model) = model_of(
            vec![thread(0, "main"), thread(7, "fiber")],
            vec![vec![100, 200], vec![300]],
            vec![
                sample(0, 0, 0, true),
                sample(1_000_000, 0, 1, true),
                sample(3_000_000, 0, 0, false),
            ],
            &[
                vec![osp_frame("fib", 5), osp_frame("main", 10)],
                vec![osp_frame("main", 12)],
            ],
        );
        speedscope_json(&prof, &model)
    }

    #[test]
    fn document_shape_and_shared_frames() {
        let doc = fixture();
        assert_eq!(doc.get("$schema").and_then(Value::as_str), Some(SCHEMA));
        assert_eq!(doc.get("exporter").and_then(Value::as_str), Some("osprey"));
        let frames = doc
            .pointer("/shared/frames")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(frames.len(), 3);
        let fib = frames.first().unwrap();
        assert_eq!(fib.get("name").and_then(Value::as_str), Some("fib"));
        assert_eq!(
            fib.get("file").and_then(Value::as_str),
            Some("/src/app.osp")
        );
        assert_eq!(fib.get("line").and_then(Value::as_u64), Some(5));
    }

    #[test]
    fn only_threads_with_samples_get_profiles() {
        let profiles = fixture();
        let profiles = profiles.get("profiles").and_then(Value::as_array).unwrap();
        assert_eq!(profiles.len(), 1);
        let main = profiles.first().unwrap();
        assert_eq!(main.get("name").and_then(Value::as_str), Some("main"));
        assert_eq!(main.get("type").and_then(Value::as_str), Some("sampled"));
        assert_eq!(main.get("unit").and_then(Value::as_str), Some("seconds"));
        assert_eq!(main.get("startValue").and_then(Value::as_u64), Some(0));
    }

    #[test]
    fn samples_are_root_first_and_include_waiting_stacks() {
        let doc = fixture();
        let samples = doc
            .pointer("/profiles/0/samples")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(samples.len(), 3);
        // Raw stack 0 is leaf-first [fib, main] -> exported root-first.
        assert_eq!(samples.first().unwrap(), &json!([1, 0]));
        assert_eq!(samples.get(2).unwrap(), &json!([1, 0]));
    }

    #[test]
    fn weights_are_deltas_with_first_at_one_over_rate() {
        let doc = fixture();
        let weights = doc
            .pointer("/profiles/0/weights")
            .and_then(Value::as_array)
            .unwrap();
        let values: Vec<f64> = weights.iter().filter_map(Value::as_f64).collect();
        assert_eq!(values.len(), 3);
        let expected = [0.001, 0.001, 0.002]; // 1/1000 Hz, then 1ms, then 2ms
        for (actual, want) in values.iter().zip(expected) {
            assert!((actual - want).abs() < 1e-12, "{values:?}");
        }
        let end = doc
            .pointer("/profiles/0/endValue")
            .and_then(Value::as_f64)
            .unwrap();
        assert!((end - 0.004).abs() < 1e-12);
    }
}
