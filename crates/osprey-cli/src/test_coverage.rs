//! Line coverage for `osprey test --coverage`. Implements
//! [TESTING-COVERAGE-CLI], [TESTING-COVERAGE-DUMP] and [TESTING-COVERAGE-JSON]
//! (docs/specs/0027-TestingFramework.md).
//!
//! One suite at a time: find where its instrumented binary dropped its dump,
//! read the dump STRICTLY, fold it into the merged report, and print the rate.
//! Every rule here exists because the alternative is a run that prints a
//! percentage nobody measured — a corrupt dump read as a small one reports a
//! HIGHER percentage over a smaller universe, and reads exactly like success.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// One suite's parsed coverage dump: flattened line → hit count
/// [TESTING-COVERAGE-DUMP].
pub(crate) type LineHits = BTreeMap<u32, u64>;

/// Where one suite's coverage dump lands (the scratch dir the compiled
/// binaries already use).
pub(crate) fn coverage_dump_path(file: &Path) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}.oscov.txt",
        crate::scratch_stem(&file.display().to_string())
    ))
}

/// Parse one suite's dump into the merged report and print its line rate.
/// `false` when the suite produced no usable evidence — a missing, unreadable,
/// malformed or EMPTY dump. Coverage that cannot be evidenced must fail the
/// command rather than print to stderr and let the aggregate row stand in for
/// evidence nobody produced ([TESTING-COVERAGE]).
pub(crate) fn collect_suite_coverage(
    file: &Path,
    dump: &Path,
    report: &mut BTreeMap<String, LineHits>,
    quiet: bool,
) -> bool {
    let Some(hits) = parse_dump(dump) else {
        eprintln!("osprey test: no coverage dump for {}", file.display());
        return false;
    };
    if hits.is_empty() {
        eprintln!(
            "osprey test: empty coverage dump for {}; no coverable lines were recorded",
            file.display()
        );
        let _ = std::fs::remove_file(dump);
        return false;
    }
    let _ = std::fs::remove_file(dump);
    let (covered, total) = line_rate(&hits);
    if !quiet {
        println!(
            "# coverage: {} ({covered}/{total} lines) {}",
            percent(covered, total),
            file.display()
        );
    }
    let _ = report.insert(file.display().to_string(), hits);
    true
}

/// The `[TESTING-COVERAGE-DUMP]` header and completion footer prefix. The
/// footer carries the row count the writer committed to, and exists so that a
/// dump truncated after a VALID PREFIX is detectable: without it, a file
/// missing its last thousand rows parses cleanly and reports a higher
/// percentage over a smaller universe — evidence of partial work, presented as
/// a complete result.
const DUMP_HEADER: &str = "# osprey-coverage v2";
const DUMP_FOOTER_PREFIX: &str = "# rows ";

/// Read a `[TESTING-COVERAGE-DUMP]` file: the header, one `<line> <hits>` row
/// per coverable line, then `# rows <n>`.
///
/// EVERY departure rejects the whole file. Skipping a malformed row is the
/// failure mode this exists to end: one good row and one corrupt one used to
/// report `100.0% (1/1)`, because the row that could not be read simply left
/// the denominator. A reader that discards what it cannot understand cannot
/// tell a complete dump from a corpse.
fn parse_dump(path: &Path) -> Option<LineHits> {
    let text = std::fs::read_to_string(path).ok()?;
    // Every record the writer emits is newline-TERMINATED, so a file that does
    // not end in one stopped mid-record — and `lines()` cannot see that, because
    // it yields a final unterminated fragment exactly as it yields a whole line.
    //
    // What this catches is narrow and worth stating precisely, because the
    // obvious story is wrong: truncation removes a SUFFIX, so every row before
    // the footer survives, and a footer whose digits were cut short always
    // declares FEWER rows than survived it — the count rule already rejects
    // that. The file only this rule refuses is the one that lost nothing but
    // the terminator: `# rows 1` where `# rows 1\n` was meant, which agrees
    // with its rows and is otherwise indistinguishable from a finished dump
    // [TESTING-COVERAGE-DUMP].
    if !text.ends_with('\n') {
        return None;
    }
    let mut rows = text.lines();
    if rows.next() != Some(DUMP_HEADER) {
        return None;
    }
    let mut hits = LineHits::new();
    while let Some(row) = rows.next() {
        match parse_dump_row(row, &mut hits) {
            Row::Recorded => {}
            // The footer ENDS the dump, so anything after it means the file is
            // not the one the writer described — two runs appended to the same
            // path, say, whose second table would otherwise be discarded in
            // silence.
            Row::Footer(declared) => {
                return (declared == hits.len() && rows.next().is_none()).then_some(hits);
            }
            Row::Malformed => return None,
        }
    }
    None // ran out of rows without the footer: the dump was never completed
}

enum Row {
    Recorded,
    Footer(usize),
    Malformed,
}

/// Classify one dump row, recording it when it is a well-formed measurement.
///
/// A duplicate line number is malformed rather than an overwrite: two counts
/// for one line means the writer emitted a table it did not have, and picking
/// either one is a guess.
fn parse_dump_row(row: &str, hits: &mut LineHits) -> Row {
    if let Some(declared) = row.strip_prefix(DUMP_FOOTER_PREFIX) {
        return declared.parse().map_or(Row::Malformed, Row::Footer);
    }
    let mut columns = row.split(' ');
    let (Some(Ok(line)), Some(Ok(count)), None) = (
        columns.next().map(str::parse::<u32>),
        columns.next().map(str::parse::<u64>),
        columns.next(),
    ) else {
        return Row::Malformed; // empty, one column, three columns, or non-numeric
    };
    // Source lines are 1-based, so line 0 names nothing. Counting it would add a
    // phantom entry to the denominator that no editor can ever highlight and no
    // test can ever cover.
    if line == 0 {
        return Row::Malformed;
    }
    match hits.insert(line, count) {
        Some(_) => Row::Malformed,
        None => Row::Recorded,
    }
}

fn line_rate(hits: &LineHits) -> (usize, usize) {
    (hits.values().filter(|h| **h > 0).count(), hits.len())
}

/// A rate over NOTHING is not perfection. Reporting `100.0%` for `0/0` reads
/// as a fully covered run to every human and every log scraper, which is the
/// exact opposite of what an empty report means ([TESTING-COVERAGE]).
fn percent(covered: usize, total: usize) -> String {
    if total == 0 {
        return String::from("n/a");
    }
    // Line counts fit u32 comfortably; saturate rather than misconvert.
    let as_f64 = |n: usize| f64::from(u32::try_from(n).unwrap_or(u32::MAX));
    let pct = as_f64(covered) / as_f64(total) * 100.0;
    format!("{pct:.1}%")
}

/// Print the aggregate `# coverage total:` row across every suite
/// [TESTING-COVERAGE-CLI].
pub(crate) fn report_total(report: &BTreeMap<String, LineHits>) {
    let (covered, total) = report
        .values()
        .map(line_rate)
        .fold((0, 0), |(c, t), (sc, st)| (c + sc, t + st));
    println!(
        "# coverage total: {} ({covered}/{total} lines)",
        percent(covered, total)
    );
}

/// Write the merged machine-readable report the editor integration consumes
/// [TESTING-COVERAGE-JSON]: `{"files":{"<path>":{"lines":{"<line>":hits}}}}`.
/// `false` when the requested JSON could not be written.
pub(crate) fn write_coverage_json(out: &str, report: &BTreeMap<String, LineHits>) -> bool {
    let files = report
        .iter()
        .map(|(file, hits)| {
            let lines = hits
                .iter()
                .map(|(line, count)| format!("\"{line}\":{count}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("{}:{{\"lines\":{{{lines}}}}}", json_string(file))
        })
        .collect::<Vec<_>>()
        .join(",");
    if let Err(e) = std::fs::write(out, format!("{{\"files\":{{{files}}}}}")) {
        eprintln!("osprey test: cannot write coverage json {out}: {e}");
        return false;
    }
    true
}

/// JSON string encoding over the WHOLE string domain, not the part that seemed
/// likely.
///
/// This used to escape quotes and backslashes only, on the stated grounds that
/// discovery could not produce a control character in a path. On Unix it can: a
/// file name may contain anything but `/` and NUL, tabs and newlines included,
/// and the walk that finds `*.test.osp` does not reject them. RFC 8259 requires
/// every U+0000–U+001F to be escaped, so one such path made the writer emit
/// invalid JSON and then report that it had successfully produced the
/// machine-readable artifact the caller asked for.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            // Everything else below U+0020 has no short form and must go out as
            // a \u escape; above it, the character is legal JSON as written.
            // Writing into a String cannot fail; the Result exists because the
            // same trait also serves sinks that can.
            control if control < ' ' => {
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            plain => out.push(plain),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_dump_path_is_stable_and_scratch_scoped() {
        let file = Path::new("/proj/tests/math_test.osp");
        let dump = coverage_dump_path(file);
        assert!(dump.starts_with(std::env::temp_dir()));
        let name = dump.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(name.ends_with(".oscov.txt"), "dump name: {name}");
        // Same suite → same dump path (the run and the collector must agree).
        assert_eq!(dump, coverage_dump_path(file));
    }

    // [TESTING-COVERAGE-DUMP] parsing, rates, and the JSON shape the editor
    // integration reads [TESTING-COVERAGE-JSON].
    #[test]
    fn dump_parsing_rates_and_json_round_trip() {
        let dir = std::env::temp_dir().join(format!("osprey-cov-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let dump = dir.join("suite.oscov.txt");
        std::fs::write(&dump, "# osprey-coverage v2\n3 2\n7 0\n12 1\n# rows 3\n")
            .expect("write dump");
        let hits = parse_dump(&dump).expect("parse");
        assert_eq!(line_rate(&hits), (2, 3));
        assert_eq!(percent(2, 3), "66.7%");
        // No coverable lines is "n/a": a rate over nothing is not 100%.
        assert_eq!(percent(0, 0), "n/a");
        assert_eq!(percent(0, 3), "0.0%");
        assert_eq!(percent(3, 3), "100.0%");

        // A dump without the header is rejected, not misread.
        std::fs::write(&dump, "3 2\n").expect("rewrite");
        assert!(parse_dump(&dump).is_none());

        // A path may legally contain a tab, a newline, and a raw control
        // character. RFC 8259 requires each of them escaped; emitting them raw
        // produced invalid JSON while the writer reported success.
        let hostile = "odd\tname\nwith\u{1}control\\and\"quote.test.osp";
        let mut report = BTreeMap::new();
        let _ = report.insert(String::from(hostile), hits);
        let json = dir.join("cov.json");
        assert!(
            write_coverage_json(&json.display().to_string(), &report),
            "a writable path must report success"
        );
        let text = std::fs::read_to_string(&json).expect("read json");
        assert_eq!(
            text,
            "{\"files\":{\"odd\\tname\\nwith\\u0001control\\\\and\\\"quote.test.osp\":\
             {\"lines\":{\"3\":2,\"7\":0,\"12\":1}}}}"
        );
        // And the artifact really is JSON: no raw control byte survived into it.
        assert!(
            !text.chars().any(|c| c < ' '),
            "a control character reached the artifact unescaped: {text:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    // [TESTING-COVERAGE] Absent evidence must FAIL the command, not print to
    // stderr and let the run exit green. Every way a dump can be useless is a
    // `false` return, asserted here rather than inferred from stderr text.
    // Each case below is chosen to be rejected by ONE rule, so deleting that
    // rule turns this test red rather than leaving it green on a second
    // objection the same bytes happen to also raise. The successful path is
    // asserted alongside them, so a change that made collection always fail --
    // or always succeed -- cannot pass either.
    #[test]
    fn coverage_evidence_that_cannot_be_read_fails_collection() {
        let dir = std::env::temp_dir().join(format!("osprey-cov-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let suite = Path::new("suite.test.osp");
        let dump = dir.join("evidence.oscov.txt");

        let collect = |report: &mut BTreeMap<String, LineHits>| {
            collect_suite_coverage(suite, &dump, report, true)
        };

        let mut report = BTreeMap::new();
        assert!(!collect(&mut report), "a dump that was never written");
        assert!(report.is_empty(), "nothing may enter the report");

        // Every shape that is not a COMPLETE, INTERNALLY CONSISTENT dump. The
        // mixed case is the one that mattered: `3 1` plus one corrupt row used
        // to report `100.0% (1/1)`, because the row that could not be read left
        // the denominator instead of failing the run. A truncated writer, a
        // full disk and a killed process all produce exactly that file.
        for (label, body) in [
            ("no header", "3 2\n"),
            ("truncated header", "# osprey-coverage\n3 2\n"),
            ("a later version", "# osprey-coverage v3\n3 2\n# rows 1\n"),
            ("empty file", ""),
            (
                "a valid row beside a corrupt one",
                "# osprey-coverage v2\n3 1\n2 garbage\n# rows 2\n",
            ),
            (
                "a valid prefix with no footer",
                "# osprey-coverage v2\n3 1\n4 0\n",
            ),
            (
                "a footer that miscounts its own rows",
                "# osprey-coverage v2\n3 1\n4 0\n# rows 5\n",
            ),
            (
                "a footer whose count is not a number",
                "# osprey-coverage v2\n3 1\n# rows many\n",
            ),
            (
                "the same line counted twice",
                "# osprey-coverage v2\n3 1\n3 2\n# rows 2\n",
            ),
            // The case above is rejected TWICE OVER — the duplicate, and then
            // the footer disagreeing with the one entry a de-duplicating reader
            // would be left holding — so it survives deleting the duplicate
            // rule. This one is rejected ONLY by that rule: a reader that let
            // the second row overwrite the first would count `{3: 2}`, agree
            // with the footer, and report 100.0% for a table its writer never
            // had.
            (
                "the same line counted twice under a footer that expects one",
                "# osprey-coverage v2\n3 1\n3 2\n# rows 1\n",
            ),
            (
                "a row carrying a third column",
                "# osprey-coverage v2\n3 1 extra\n# rows 1\n",
            ),
            (
                "a negative hit count",
                "# osprey-coverage v2\n3 -1\n# rows 1\n",
            ),
            (
                "a line number of zero",
                "# osprey-coverage v2\n0 1\n# rows 1\n",
            ),
            (
                "a negative line number",
                "# osprey-coverage v2\n-3 1\n# rows 1\n",
            ),
            ("a blank row", "# osprey-coverage v2\n3 1\n\n# rows 1\n"),
            (
                "rows after the footer",
                "# osprey-coverage v2\n3 1\n# rows 1\n4 0\n",
            ),
            // The ONE shape only the terminator rule refuses: a footer that
            // agrees with its rows and is missing nothing but its newline.
            // Every other unterminated file is already rejected by the count
            // or the missing-footer rule, so this is the case that turns red
            // when the guard is deleted.
            (
                "a footer with no terminating newline",
                "# osprey-coverage v2\n3 1\n# rows 1",
            ),
        ] {
            std::fs::write(&dump, body).expect("write dump");
            assert!(!collect(&mut report), "{label} must not count as evidence");
            assert!(report.is_empty(), "{label} must not enter the report");
        }

        // The header and footer alone: well-formed, but it evidences no
        // coverable line.
        std::fs::write(&dump, "# osprey-coverage v2\n# rows 0\n").expect("write dump");
        assert!(!collect(&mut report), "a dump with no rows is no evidence");
        assert!(report.is_empty());
        assert!(
            !dump.exists(),
            "an empty dump is cleaned up, not left behind"
        );

        // A dump of nothing but junk is a failure for the same reason: the
        // rows are rejected, not skipped past.
        std::fs::write(&dump, "# osprey-coverage v2\nnot a row\n9\n").expect("write dump");
        assert!(!collect(&mut report), "unparseable rows are not evidence");
        assert!(report.is_empty());

        // A complete dump is evidence: collection succeeds, the report gains
        // the suite, and the dump is consumed so a later run cannot reuse it.
        std::fs::write(&dump, "# osprey-coverage v2\n4 1\n5 0\n# rows 2\n").expect("write dump");
        assert!(collect(&mut report), "a well-formed dump must be accepted");
        assert_eq!(report.len(), 1);
        assert_eq!(
            line_rate(report.get("suite.test.osp").expect("suite entry")),
            (1, 2)
        );
        assert!(!dump.exists(), "a consumed dump must not survive the run");

        // A JSON artifact that cannot be written is a failure too: exiting
        // green would claim a file the caller cannot read.
        assert!(
            !write_coverage_json(&dir.display().to_string(), &report),
            "writing over a directory must report failure"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
