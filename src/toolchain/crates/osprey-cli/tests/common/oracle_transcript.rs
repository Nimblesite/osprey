//! Strict acceptance of independently generated package transcripts. [PORT-SUITE]

pub(crate) fn successful_oracle(output: &str) -> bool {
    let lines: Vec<_> = output.lines().collect();
    let Some((summary, outcomes)) = lines.split_last() else {
        return false;
    };
    let tap = summary.starts_with("# tests=");
    let count = if tap {
        summary
            .strip_prefix("# tests=")
            .and_then(|tail| tail.split(' ').next())
    } else {
        summary.strip_suffix(" passed, 0 failed")
    }
    .and_then(|text| text.parse::<usize>().ok());
    let Some(count) = count.filter(|count| *count > 0) else {
        return false;
    };
    let rows = outcomes
        .iter()
        .filter(|line| {
            line.strip_prefix("ok ")
                .is_some_and(|name| !name.trim().is_empty())
        })
        .copied()
        .collect::<Vec<_>>();
    rows.len() == count
        && !outcomes.iter().any(|line| {
            ["FAIL", "not ok"].iter().any(|marker| {
                line.strip_prefix(*marker)
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with(' '))
            })
        })
        && (!tap || successful_tap(summary, outcomes, &rows, count))
}

/// Native TAP must prove ordered tests, one plan and zero failed/skipped cases.
fn successful_tap(summary: &str, outcomes: &[&str], rows: &[&str], count: usize) -> bool {
    let plan = format!("1..{count}");
    summary == format!("# tests={count} passed={count} failed=0 skipped=0")
        && outcomes.iter().filter(|line| **line == plan).count() == 1
        && outcomes
            .iter()
            .all(|line| line.starts_with("ok ") || line.starts_with('#') || *line == plan)
        && rows.iter().enumerate().all(|(index, line)| {
            line.strip_prefix(&format!("ok {} - ", index + 1))
                .is_some_and(|name| {
                    !name.trim().is_empty() && !name.contains("# SKIP") && !name.contains("# TODO")
                })
        })
}

#[test]
fn oracle_success_requires_real_passing_assertions() {
    assert!(successful_oracle("ok one\nok two\n2 passed, 0 failed\n"));
    assert!(successful_oracle(
        "ok multiline\npattern\n1 passed, 0 failed\n"
    ));
    assert!(successful_oracle(
        "ok multiline\nnot okay\n1 passed, 0 failed\n"
    ));
    assert!(successful_oracle(
        "ok 1 - real HTTP requests\n1..1\n# tests=1 passed=1 failed=0 skipped=0\n"
    ));
    assert!(successful_oracle(
        "ok 1 - first\nok 2 - second\n1..2\n# tests=2 passed=2 failed=0 skipped=0\n"
    ));
    for bad in [
        "1..0\n# tests=0 passed=0 failed=0 skipped=0\n",
        "ok 1 - first\n1..2\n# tests=2 passed=2 failed=0 skipped=0\n",
        "ok 1 - first\n1..1\n# tests=1 passed=0 failed=1 skipped=0\n",
        "not ok 1 - first\n1..1\n# tests=1 passed=1 failed=0 skipped=0\n",
        "ok 1 - first # SKIP\n1..1\n# tests=1 passed=1 failed=0 skipped=0\n",
        "ok 1 - first # TODO\n1..1\n# tests=1 passed=1 failed=0 skipped=0\n",
        "ok 1 - first\nok 1 - duplicate\n1..2\n# tests=2 passed=2 failed=0 skipped=0\n",
        "ok 1 - \n1..1\n# tests=1 passed=1 failed=0 skipped=0\n",
        "ok 1 - first\n# tests=1 passed=1 failed=0 skipped=0\n",
        "ok 1 - first\n1..1\n1..1\n# tests=1 passed=1 failed=0 skipped=0\n",
        "ok 1 - first\nFAIL hidden\n1..1\n# tests=1 passed=1 failed=0 skipped=0\n",
        "",
        "0 passed, 0 failed\n",
        "FAIL one\n1 passed, 0 failed\n",
        "ok one\n1 passed, 1 failed\n",
        "ok one\n2 passed, 0 failed\n",
        "ok \n1 passed, 0 failed\n",
        "ok one\nnot ok two\n1 passed, 0 failed\n",
        "ok one\nFAIL\n1 passed, 0 failed\n",
        "ok one\nnot ok\n1 passed, 0 failed\n",
    ] {
        assert!(!successful_oracle(bad), "accepted invalid oracle: {bad}");
    }
}
