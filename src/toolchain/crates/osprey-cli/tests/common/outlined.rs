//! Which declarations the `--symbols` outline reports, so an annotation gate
//! judges only what its oracle can see.

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Whether line `index` declares something the outline reports: it sits at the
/// top level, or its nearest less-indented line opens a `module`. A local
/// binding never reaches `--symbols`, so the oracle cannot see what deleting
/// its annotation changes: `chosen : float32` inside a body still compiles
/// without it and outlines identically, but then computes in `float`.
pub(crate) fn outlined(lines: &[&str], index: usize) -> bool {
    let Some(own) = lines.get(index).map(|line| indent(line)) else {
        return false;
    };
    own == 0
        || lines
            .get(..index)
            .unwrap_or_default()
            .iter()
            .rev()
            .find(|line| !line.trim().is_empty() && indent(line) < own)
            .is_some_and(|parent| {
                let opener = parent.trim_start();
                opener
                    .strip_prefix("export ")
                    .unwrap_or(opener)
                    .starts_with("module ")
            })
}
