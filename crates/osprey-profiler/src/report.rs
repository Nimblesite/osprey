//! Terminal report [PROF-CLI-REPORT]: header, fiber-state split, a top-10
//! self/total table with Unicode eighth-block bars and perf-style color
//! thresholds, a √n low-confidence note, and the export footer. No calls
//! column — a sampling profiler cannot honestly report call counts.

use crate::model::{pct, samples_to_secs, u64_to_f64, FiberStat, FuncStat, Model};
use std::path::Path;

/// Rows shown in the hot-function table.
const TOP_FUNCTIONS: usize = 10;
/// Width of the bar gutter, in terminal cells.
const BAR_CELLS: u64 = 6;
/// Eighth-block resolution per cell.
const EIGHTHS_PER_CELL: u64 = 8;
/// Below this many on-CPU samples the percentages get a confidence warning.
const LOW_SAMPLE_THRESHOLD: u64 = 100;
/// Self% at or above this renders red and bold.
const HOT_PCT: f64 = 5.0;
/// Self% at or above this renders yellow.
const WARM_PCT: f64 = 0.5;
/// ANSI: bold red / yellow / dim / reset.
const RED_BOLD: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[33m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
/// Partial blocks by eighth (1/8 … 7/8); a full cell is `█`.
const PARTIAL_BLOCKS: [&str; 7] = ["▏", "▎", "▍", "▌", "▋", "▊", "▉"];
const FULL_BLOCK: &str = "█";

/// Render the whole report for `source`, referencing `<stem>.*` exports.
pub(crate) fn render_report(source: &str, stem: &str, model: &Model, color: bool) -> String {
    let mut out = format!(
        "{}\n{}\n\n{}",
        header_line(source, model),
        fibers_line(model),
        table(model, color)
    );
    if model.oncpu_samples < LOW_SAMPLE_THRESHOLD {
        out.push_str(&low_sample_note(model.oncpu_samples));
    }
    out.push('\n');
    out.push_str(&footer(stem));
    out
}

/// `fib.osp · 4.21s wall · 3.97s CPU · 4182 samples @ 997Hz · 3 fibers`.
fn header_line(source: &str, model: &Model) -> String {
    let name = Path::new(source)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(source);
    let fibers = model.fibers.len();
    let noun = if fibers == 1 { "fiber" } else { "fibers" };
    format!(
        "{name} · {:.2}s wall · {:.2}s CPU · {} samples @ {}Hz · {fibers} {noun}",
        model.wall_seconds, model.cpu_seconds, model.sample_count, model.rate_hz
    )
}

/// `fibers: main 87% on-cpu · fiber-2 45% on-cpu · fiber-3 waiting`.
fn fibers_line(model: &Model) -> String {
    let parts: Vec<String> = model.fibers.iter().map(fiber_state).collect();
    format!("fibers: {}", parts.join(" · "))
}

/// One fiber's state summary, over ALL of its samples.
fn fiber_state(fiber: &FiberStat) -> String {
    if fiber.oncpu_samples == 0 {
        return format!("{} waiting", fiber.label);
    }
    format!(
        "{} {:.0}% on-cpu",
        fiber.label,
        pct(fiber.oncpu_samples, fiber.samples)
    )
}

/// Header plus the top-10 rows.
fn table(model: &Model, color: bool) -> String {
    let header = format!(
        "{:>13}{:>8}{:>9}{:>9}  FUNCTION  LOCATION\n",
        "SELF%", "TOTAL%", "SELF", "TOTAL"
    );
    model
        .funcs
        .iter()
        .take(TOP_FUNCTIONS)
        .fold(header, |mut out, func| {
            out.push_str(&table_row(model, func, color));
            out
        })
}

/// One colored table row behind its bar gutter.
fn table_row(model: &Model, func: &FuncStat, color: bool) -> String {
    let self_pct = pct(func.self_samples, model.oncpu_samples);
    let (on, off) = row_colors(self_pct, color);
    format!(
        "{}{on}{self_pct:>6.1}%{:>7.1}%{:>8.2}s{:>8.2}s  {}{}{off}\n",
        bar(func.self_samples, model.oncpu_samples),
        pct(func.total_samples, model.oncpu_samples),
        samples_to_secs(func.self_samples, model.rate_hz),
        samples_to_secs(func.total_samples, model.rate_hz),
        func.name,
        location(func),
    )
}

/// Perf-style thresholds: ≥5% red+bold, ≥0.5% yellow, else dim.
fn row_colors(self_pct: f64, color: bool) -> (&'static str, &'static str) {
    if !color {
        return ("", "");
    }
    if self_pct >= HOT_PCT {
        (RED_BOLD, RESET)
    } else if self_pct >= WARM_PCT {
        (YELLOW, RESET)
    } else {
        (DIM, RESET)
    }
}

/// `  fib.osp:142` — basename only; empty for unlocated runtime frames.
fn location(func: &FuncStat) -> String {
    if func.file.is_empty() {
        return String::new();
    }
    let base = Path::new(&func.file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&func.file);
    if func.line == 0 {
        format!("  {base}")
    } else {
        format!("  {base}:{}", func.line)
    }
}

/// A 6-cell eighth-block bar proportional to the self share (100% = full).
fn bar(self_samples: u64, oncpu: u64) -> String {
    let eighths = bar_eighths(self_samples, oncpu);
    let full = usize::try_from(eighths / EIGHTHS_PER_CELL).unwrap_or(0);
    let partial = usize::try_from(eighths % EIGHTHS_PER_CELL).unwrap_or(0);
    let mut cells = FULL_BLOCK.repeat(full);
    if let Some(block) = partial.checked_sub(1).and_then(|i| PARTIAL_BLOCKS.get(i)) {
        cells.push_str(block);
    }
    let used = full + usize::from(partial > 0);
    let width = usize::try_from(BAR_CELLS).unwrap_or(used);
    cells + &" ".repeat(width.saturating_sub(used))
}

/// `round(self / oncpu * 48)` in pure integer math (no lossy casts).
fn bar_eighths(self_samples: u64, oncpu: u64) -> u64 {
    if oncpu == 0 {
        return 0;
    }
    let scale = BAR_CELLS * EIGHTHS_PER_CELL;
    (self_samples.saturating_mul(scale * 2).saturating_add(oncpu) / (oncpu * 2)).min(scale)
}

/// The √n error rule, spelled out when the sample count is too small.
fn low_sample_note(oncpu: u64) -> String {
    let margin = if oncpu == 0 {
        100.0
    } else {
        100.0 * u64_to_f64(oncpu).sqrt() / u64_to_f64(oncpu)
    };
    format!(
        "note: only {oncpu} on-CPU samples — treat percentages as ±{margin:.1}% (run longer for confidence)\n"
    )
}

/// Where the exports landed and how to view them.
fn footer(stem: &str) -> String {
    format!(
        "profile: {stem}.speedscope.json · {stem}.cpuprofile · {stem}.folded\nview: https://speedscope.app (drag the file) or open {stem}.cpuprofile in VS Code\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::build_model;
    use crate::raw::Profile;
    use crate::symbolize::SymFrame;
    use crate::testutil::{model_of, osp_frame, profile, sample, thread};

    /// 200 on-CPU samples on main (no low-sample note): 100 leaf on parse
    /// (50%), 2 leaf on emit (1%), 98 leaf on tick (49%, runtime); plus a
    /// waiting-only fiber.
    fn fixture() -> Profile {
        let mut samples = Vec::new();
        samples.extend((0..100_u64).map(|i| sample(i * 1_000_000, 0, 0, true)));
        samples.extend((100..102_u64).map(|i| sample(i * 1_000_000, 0, 1, true)));
        samples.extend((102..200_u64).map(|i| sample(i * 1_000_000, 0, 2, true)));
        samples.push(sample(200_000_000, 1, 2, false));
        profile(
            vec![thread(0, "main"), thread(3, "fiber")],
            vec![vec![100, 300], vec![200, 300], vec![400]],
            samples,
        )
    }

    fn sym_stacks() -> Vec<Vec<SymFrame>> {
        vec![
            vec![osp_frame("parse", 142), osp_frame("main", 3)],
            vec![osp_frame("emit", 9), osp_frame("main", 3)],
            vec![SymFrame::new("tick", "/rt/loop.c", 7)],
        ]
    }

    fn render(color: bool) -> String {
        let model = build_model(&fixture(), &sym_stacks());
        render_report("/abs/dir/fib.osp", "fib", &model, color)
    }

    /// A model with `count` on-CPU single-frame samples.
    fn tiny_model(count: u64) -> Model {
        let (_, model) = model_of(
            vec![thread(0, "main")],
            vec![vec![100]],
            (0..count).map(|i| sample(i, 0, 0, true)).collect(),
            &[vec![osp_frame("fib", 5)]],
        );
        model
    }

    #[test]
    fn header_uses_the_source_basename_and_totals() {
        let report = render(false);
        let header = report.lines().next().unwrap();
        assert_eq!(
            header,
            "fib.osp · 0.20s wall · 0.20s CPU · 201 samples @ 1000Hz · 2 fibers"
        );
    }

    #[test]
    fn fiber_line_reports_oncpu_share_and_waiting() {
        let report = render(false);
        assert!(
            report.contains("fibers: main 100% on-cpu · fiber-3 waiting"),
            "{report}"
        );
    }

    #[test]
    fn table_has_header_rows_bars_and_locations_without_color() {
        let report = render(false);
        assert!(
            report.contains("SELF%  TOTAL%     SELF    TOTAL  FUNCTION  LOCATION"),
            "{report}"
        );
        // parse: 100/200 self = 50.0%, total 50.0%, 0.10s at 1000 Hz; the
        // 50% bar is exactly 3 of 6 cells.
        assert!(
            report.contains("███     50.0%   50.0%    0.10s    0.10s  parse  app.osp:142"),
            "{report}"
        );
        // tick keeps its runtime location; main has self 0 and an empty bar.
        assert!(report.contains("tick  loop.c:7"), "{report}");
        assert!(
            report.contains("         0.0%   51.0%    0.00s    0.10s  main  app.osp:3"),
            "{report}"
        );
        assert!(!report.contains('\x1b'));
    }

    #[test]
    fn color_thresholds_wrap_rows() {
        let report = render(true);
        assert!(
            report.contains("\x1b[1;31m"),
            "hot row must be red+bold: {report}"
        );
        assert!(
            report.contains("\x1b[33m"),
            "warm row must be yellow: {report}"
        );
        assert!(report.contains("\x1b[2m"), "cold row must be dim: {report}");
        assert!(report.contains(RESET));
    }

    #[test]
    fn no_low_sample_note_at_or_above_the_threshold() {
        assert!(!render(false).contains("note: only"));
    }

    #[test]
    fn low_sample_note_states_the_sqrt_n_margin() {
        let report = render_report("fib.osp", "fib", &tiny_model(25), false);
        // sqrt(25)/25 = 20%.
        assert!(
            report.contains("note: only 25 on-CPU samples — treat percentages as ±20.0%"),
            "{report}"
        );
    }

    #[test]
    fn zero_oncpu_samples_report_full_uncertainty() {
        let report = render_report("fib.osp", "fib", &tiny_model(0), false);
        assert!(report.contains("±100.0%"), "{report}");
        assert!(report.contains("0 samples @ 1000Hz · 1 fiber"), "{report}");
    }

    #[test]
    fn footer_names_the_exports_and_viewers() {
        let report = render(false);
        assert!(report.contains("profile: fib.speedscope.json · fib.cpuprofile · fib.folded"));
        assert!(report.contains(
            "view: https://speedscope.app (drag the file) or open fib.cpuprofile in VS Code"
        ));
    }

    #[test]
    fn bars_scale_by_eighths() {
        assert_eq!(bar(0, 100), "      ");
        assert_eq!(bar(100, 100), "██████");
        assert_eq!(bar(50, 100), "███   ");
        // 42.1% of 48 eighths ≈ 20.2 -> 2 full cells + 4/8 partial.
        assert_eq!(bar(421, 1000), "██▌   ");
        assert_eq!(bar(1, 0), "      ");
        // Every partial glyph is reachable.
        for (index, glyph) in PARTIAL_BLOCKS.iter().enumerate() {
            let eighths = u64::try_from(index).unwrap() + 1;
            assert_eq!(bar(eighths, 48), format!("{glyph}     "));
        }
    }
}
