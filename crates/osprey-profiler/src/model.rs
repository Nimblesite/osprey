//! Aggregation of symbolized samples into the profile model every exporter
//! and the terminal report consume: interned frames, per-function self/total
//! stats (on-CPU only, recursion-deduplicated), hot `.osp` lines, and the
//! per-fiber on-CPU/waiting split.

use crate::raw::{Profile, Sample, Thread};
use crate::symbolize::{FrameKind, SymFrame};
use std::collections::{BTreeMap, BTreeSet};

/// Per-function statistics from on-CPU samples only. `self_samples` counts
/// leaf frames; `total_samples` counts each sample at most once per function
/// even when recursion repeats the function within one stack.
#[derive(Debug)]
pub(crate) struct FuncStat {
    /// Function name.
    pub name: String,
    /// Source file (empty when unknown).
    pub file: String,
    /// Smallest known 1-based line the function was seen at; 0 when unknown.
    pub line: u32,
    /// User vs runtime classification.
    pub kind: FrameKind,
    /// On-CPU samples with this function as the leaf frame.
    pub self_samples: u64,
    /// On-CPU samples with this function anywhere on the stack.
    pub total_samples: u64,
}

/// One hot source line: leaf frames of on-CPU samples, `.osp` files only.
#[derive(Debug)]
pub(crate) struct LineStat {
    /// Source file path.
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// On-CPU leaf samples attributed to the line.
    pub samples: u64,
}

/// Per-fiber sample split, computed from ALL samples (on-CPU and waiting).
#[derive(Debug)]
pub(crate) struct FiberStat {
    /// The fiber id the thread registered with (-1 for effect threads).
    pub id: i64,
    /// Display label: `main`, `fiber-<id>`, `effect-<row index>`.
    pub label: String,
    /// All samples of this fiber.
    pub samples: u64,
    /// On-CPU samples of this fiber.
    pub oncpu_samples: u64,
}

/// The aggregated profile model shared by every exporter and the report.
#[derive(Debug)]
pub(crate) struct Model {
    /// Interned unique `(name, file, line)` frames.
    pub frames: Vec<SymFrame>,
    /// Per raw stack: interned frame ids, leaf-first like the input.
    pub stacks: Vec<Vec<usize>>,
    /// Function stats sorted by descending self samples.
    pub funcs: Vec<FuncStat>,
    /// Hot lines sorted by descending samples.
    pub lines: Vec<LineStat>,
    /// One entry per thread row of the raw profile.
    pub fibers: Vec<FiberStat>,
    /// Last sample time minus first sample time (0 when under 2 samples).
    pub wall_seconds: f64,
    /// `oncpu_samples / rate_hz`.
    pub cpu_seconds: f64,
    /// All samples, on-CPU and waiting.
    pub sample_count: u64,
    /// On-CPU samples only.
    pub oncpu_samples: u64,
    /// Nominal sampling rate.
    pub rate_hz: u64,
    /// Samples the runtime dropped.
    pub dropped: u64,
}

impl Model {
    /// Frame ids of `stack` in ROOT-FIRST order (the raw stack is
    /// leaf-first).
    pub(crate) fn root_first(&self, stack: usize) -> Vec<usize> {
        self.stacks
            .get(stack)
            .map(|ids| ids.iter().rev().copied().collect())
            .unwrap_or_default()
    }
}

/// Aggregate the validated profile plus its symbolized stacks.
pub(crate) fn build_model(profile: &Profile, sym_stacks: &[Vec<SymFrame>]) -> Model {
    let (frames, stacks) = intern_frames(sym_stacks);
    let oncpu_samples = profile.samples.iter().filter(|s| s.on_cpu).count();
    let oncpu = u64::try_from(oncpu_samples).unwrap_or(u64::MAX);
    Model {
        frames,
        stacks,
        funcs: func_stats(profile, sym_stacks),
        lines: line_stats(profile, sym_stacks),
        fibers: fiber_stats(profile),
        wall_seconds: wall_seconds(&profile.samples),
        cpu_seconds: samples_to_secs(oncpu, profile.rate_hz),
        sample_count: u64::try_from(profile.samples.len()).unwrap_or(u64::MAX),
        oncpu_samples: oncpu,
        rate_hz: profile.rate_hz,
        dropped: profile.dropped,
    }
}

/// Deduplicate frames by `(name, file, line)` into a shared table.
fn intern_frames(sym_stacks: &[Vec<SymFrame>]) -> (Vec<SymFrame>, Vec<Vec<usize>>) {
    let mut ids: BTreeMap<(String, String, u32), usize> = BTreeMap::new();
    let mut frames: Vec<SymFrame> = Vec::new();
    let stacks = sym_stacks
        .iter()
        .map(|stack| {
            stack
                .iter()
                .map(|frame| intern(&mut ids, &mut frames, frame))
                .collect()
        })
        .collect();
    (frames, stacks)
}

/// The id of `frame` in the shared table, inserting it on first sight.
fn intern(
    ids: &mut BTreeMap<(String, String, u32), usize>,
    frames: &mut Vec<SymFrame>,
    frame: &SymFrame,
) -> usize {
    let key = (frame.name.clone(), frame.file.clone(), frame.line);
    if let Some(&id) = ids.get(&key) {
        return id;
    }
    let id = frames.len();
    frames.push(frame.clone());
    let _ = ids.insert(key, id);
    id
}

/// Functions are identified by `(name, file)`: recursion and multiple sampled
/// lines inside one function must aggregate into a single row.
type FuncKey = (String, String);

/// Accumulator for one function while walking the samples.
#[derive(Debug, Default)]
struct FuncAcc {
    line: u32,
    kind: FrameKind,
    self_samples: u64,
    total_samples: u64,
}

/// Self/total per function, on-CPU samples only, sorted hottest-first.
fn func_stats(profile: &Profile, sym_stacks: &[Vec<SymFrame>]) -> Vec<FuncStat> {
    let mut acc: BTreeMap<FuncKey, FuncAcc> = BTreeMap::new();
    for sample in profile.samples.iter().filter(|s| s.on_cpu) {
        if let Some(stack) = sym_stacks.get(sample.stack) {
            record_sample(&mut acc, stack);
        }
    }
    let mut funcs: Vec<FuncStat> = acc
        .into_iter()
        .map(|((name, file), a)| FuncStat {
            name,
            file,
            line: a.line,
            kind: a.kind,
            self_samples: a.self_samples,
            total_samples: a.total_samples,
        })
        .collect();
    funcs.sort_by(|a, b| {
        b.self_samples
            .cmp(&a.self_samples)
            .then_with(|| b.total_samples.cmp(&a.total_samples))
            .then_with(|| a.name.cmp(&b.name))
    });
    funcs
}

/// Fold one on-CPU sample into the accumulators: total counts once per
/// function per sample (recursion dedup), self counts the leaf only.
fn record_sample(acc: &mut BTreeMap<FuncKey, FuncAcc>, stack: &[SymFrame]) {
    let mut seen: BTreeSet<FuncKey> = BTreeSet::new();
    for frame in stack {
        let key = func_key(frame);
        let entry = acc.entry(key.clone()).or_default();
        merge_meta(entry, frame);
        if seen.insert(key) {
            entry.total_samples += 1;
        }
    }
    if let Some(leaf) = stack.first() {
        if let Some(entry) = acc.get_mut(&func_key(leaf)) {
            entry.self_samples += 1;
        }
    }
}

/// `(name, file)` identity of a frame's function.
fn func_key(frame: &SymFrame) -> FuncKey {
    (frame.name.clone(), frame.file.clone())
}

/// Keep the smallest known line as the function's display line.
fn merge_meta(entry: &mut FuncAcc, frame: &SymFrame) {
    if entry.line == 0 || (frame.line != 0 && frame.line < entry.line) {
        entry.line = frame.line;
    }
    entry.kind = frame.kind;
}

/// Hot lines: leaf frames of on-CPU samples, `.osp` files with known lines.
fn line_stats(profile: &Profile, sym_stacks: &[Vec<SymFrame>]) -> Vec<LineStat> {
    let mut acc: BTreeMap<(String, u32), u64> = BTreeMap::new();
    for sample in profile.samples.iter().filter(|s| s.on_cpu) {
        let Some(leaf) = sym_stacks.get(sample.stack).and_then(|s| s.first()) else {
            continue;
        };
        if leaf.kind == FrameKind::User && leaf.line > 0 {
            *acc.entry((leaf.file.clone(), leaf.line)).or_insert(0) += 1;
        }
    }
    let mut lines: Vec<LineStat> = acc
        .into_iter()
        .map(|((file, line), samples)| LineStat {
            file,
            line,
            samples,
        })
        .collect();
    lines.sort_by(|a, b| {
        b.samples
            .cmp(&a.samples)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    lines
}

/// Per-thread sample split over ALL samples.
fn fiber_stats(profile: &Profile) -> Vec<FiberStat> {
    let mut stats: Vec<FiberStat> = profile
        .threads
        .iter()
        .enumerate()
        .map(|(index, t)| FiberStat {
            id: t.fiber,
            label: display_label(t, index),
            samples: 0,
            oncpu_samples: 0,
        })
        .collect();
    for sample in &profile.samples {
        if let Some(stat) = stats.get_mut(sample.thread) {
            stat.samples += 1;
            stat.oncpu_samples += u64::from(sample.on_cpu);
        }
    }
    stats
}

/// `main` stays bare; other labels append the fiber id (`fiber-3`). Effect
/// continuation threads register with fiber id -1 [PROF-COLLECT-REGISTRY],
/// so negative ids append the thread ROW index (`effect-2`), never
/// `effect--1`.
pub(crate) fn display_label(thread: &Thread, index: usize) -> String {
    if thread.label == "main" {
        thread.label.clone()
    } else if thread.fiber < 0 {
        format!("{}-{index}", thread.label)
    } else {
        format!("{}-{}", thread.label, thread.fiber)
    }
}

/// Wall time spanned by the sample train; 0 when under 2 samples.
fn wall_seconds(samples: &[Sample]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let first = samples.iter().map(|s| s.t_ns).min().unwrap_or(0);
    let last = samples.iter().map(|s| s.t_ns).max().unwrap_or(0);
    ns_to_secs(last.saturating_sub(first))
}

/// Lossless-for-realistic-values u64 → f64 via 32-bit halves (no `as`).
pub(crate) fn u64_to_f64(value: u64) -> f64 {
    let hi = u32::try_from(value >> 32).unwrap_or(u32::MAX);
    let lo = u32::try_from(value & u64::from(u32::MAX)).unwrap_or(u32::MAX);
    f64::from(hi) * 4_294_967_296.0 + f64::from(lo)
}

/// Nanoseconds → seconds.
pub(crate) fn ns_to_secs(ns: u64) -> f64 {
    u64_to_f64(ns) / 1e9
}

/// Sample count → estimated seconds at the nominal sampling rate.
pub(crate) fn samples_to_secs(samples: u64, rate_hz: u64) -> f64 {
    if rate_hz == 0 {
        return 0.0;
    }
    u64_to_f64(samples) / u64_to_f64(rate_hz)
}

/// `part` as a percentage of `whole` (0 when `whole` is 0).
pub(crate) fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    u64_to_f64(part) * 100.0 / u64_to_f64(whole)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{osp_frame, profile, sample, thread};

    fn approx(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    /// One stack where `fib` recurses: [fib(leaf), fib, main].
    fn recursive_model() -> Model {
        let prof = profile(
            vec![thread(0, "main")],
            vec![vec![100, 200, 300]],
            vec![sample(0, 0, 0, true), sample(1_000_000, 0, 0, true)],
        );
        let stacks = vec![vec![
            osp_frame("fib", 5),
            osp_frame("fib", 3),
            osp_frame("main", 10),
        ]];
        build_model(&prof, &stacks)
    }

    #[test]
    fn recursion_counts_total_once_per_sample() {
        let model = recursive_model();
        let fib = model.funcs.first().unwrap();
        assert_eq!(
            (fib.name.as_str(), fib.self_samples, fib.total_samples),
            ("fib", 2, 2)
        );
        let main = model.funcs.get(1).unwrap();
        assert_eq!(
            (main.name.as_str(), main.self_samples, main.total_samples),
            ("main", 0, 2)
        );
        assert_eq!(main.kind, FrameKind::User);
    }

    #[test]
    fn function_line_is_the_smallest_known_line() {
        assert_eq!(recursive_model().funcs.first().unwrap().line, 3);
    }

    #[test]
    fn interning_dedups_identical_frames_and_keeps_leaf_first_order() {
        let model = recursive_model();
        assert_eq!(model.frames.len(), 3);
        assert_eq!(model.stacks.first().unwrap().as_slice(), &[0, 1, 2]);
        assert_eq!(model.root_first(0), vec![2, 1, 0]);
        assert!(model.root_first(9).is_empty());
    }

    #[test]
    fn self_vs_total_split_across_different_leaves() {
        let prof = profile(
            vec![thread(0, "main")],
            vec![vec![100, 300], vec![300]],
            vec![
                sample(0, 0, 0, true),
                sample(1, 0, 0, true),
                sample(2, 0, 1, true),
            ],
        );
        let stacks = vec![
            vec![osp_frame("fib", 5), osp_frame("main", 10)],
            vec![osp_frame("main", 12)],
        ];
        let model = build_model(&prof, &stacks);
        let fib = model.funcs.first().unwrap();
        assert_eq!(
            (fib.name.as_str(), fib.self_samples, fib.total_samples),
            ("fib", 2, 2)
        );
        let main = model.funcs.get(1).unwrap();
        assert_eq!((main.self_samples, main.total_samples), (1, 3));
    }

    #[test]
    fn waiting_samples_never_reach_function_or_line_stats() {
        let prof = profile(
            vec![thread(0, "main")],
            vec![vec![100]],
            vec![sample(0, 0, 0, false), sample(1, 0, 0, false)],
        );
        let model = build_model(&prof, &[vec![osp_frame("fib", 5)]]);
        assert!(model.funcs.is_empty());
        assert!(model.lines.is_empty());
        assert_eq!((model.sample_count, model.oncpu_samples), (2, 0));
    }

    #[test]
    fn hot_lines_keep_only_osp_leaves_with_known_lines() {
        let prof = profile(
            vec![thread(0, "main")],
            vec![vec![100], vec![200], vec![300]],
            vec![
                sample(0, 0, 0, true),
                sample(1, 0, 0, true),
                sample(2, 0, 1, true),
                sample(3, 0, 2, true),
            ],
        );
        let stacks = vec![
            vec![osp_frame("fib", 5)],
            vec![SymFrame::new("memcpy", "/rt/string.c", 40)],
            vec![osp_frame("mystery", 0)],
        ];
        let model = build_model(&prof, &stacks);
        assert_eq!(model.lines.len(), 1);
        let hot = model.lines.first().unwrap();
        assert_eq!(
            (hot.file.as_str(), hot.line, hot.samples),
            ("/src/app.osp", 5, 2)
        );
    }

    #[test]
    fn fiber_stats_split_all_samples_and_label_fibers() {
        let prof = profile(
            vec![thread(0, "main"), thread(2, "fiber"), thread(-1, "effect")],
            vec![vec![100]],
            vec![
                sample(0, 0, 0, true),
                sample(1, 0, 0, false),
                sample(2, 1, 0, false),
                sample(3, 2, 0, true),
            ],
        );
        let model = build_model(&prof, &[vec![osp_frame("fib", 5)]]);
        let labels: Vec<&str> = model.fibers.iter().map(|f| f.label.as_str()).collect();
        // The effect thread registers with fiber id -1: its label uses the
        // thread ROW index (2), never "effect--1".
        assert_eq!(labels, ["main", "fiber-2", "effect-2"]);
        let main = model.fibers.first().unwrap();
        assert_eq!((main.id, main.samples, main.oncpu_samples), (0, 2, 1));
        let fiber = model.fibers.get(1).unwrap();
        assert_eq!((fiber.samples, fiber.oncpu_samples), (1, 0));
        assert_eq!(model.fibers.get(2).unwrap().id, -1);
    }

    #[test]
    fn wall_and_cpu_seconds() {
        let prof = profile(
            vec![thread(0, "main")],
            vec![vec![100]],
            vec![sample(2_000_000_000, 0, 0, true), sample(0, 0, 0, true)],
        );
        let model = build_model(&prof, &[vec![osp_frame("fib", 5)]]);
        approx(model.wall_seconds, 2.0);
        approx(model.cpu_seconds, 0.002); // 2 samples at the 1000 Hz test rate
    }

    #[test]
    fn single_sample_profiles_have_zero_wall_time() {
        let prof = profile(
            vec![thread(0, "main")],
            vec![vec![100]],
            vec![sample(5, 0, 0, true)],
        );
        approx(
            build_model(&prof, &[vec![osp_frame("fib", 5)]]).wall_seconds,
            0.0,
        );
    }

    #[test]
    fn conversion_helpers() {
        approx(u64_to_f64(0), 0.0);
        approx(u64_to_f64(4_294_967_296), 4_294_967_296.0);
        approx(ns_to_secs(1_500_000_000), 1.5);
        approx(samples_to_secs(997, 997), 1.0);
        approx(samples_to_secs(10, 0), 0.0);
        approx(pct(1, 8), 12.5);
        approx(pct(3, 0), 0.0);
    }
}
