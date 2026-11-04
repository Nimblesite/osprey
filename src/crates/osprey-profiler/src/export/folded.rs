//! Brendan Gregg collapsed-stacks export [PROF-CLI-RUN]: one line per unique
//! `(fiber, stack)` over ALL samples, root-first, with the fiber label as a
//! synthetic root frame — ready for inferno, flamelens, and flame-graph
//! diffing.

use crate::model::{display_label, Model};
use crate::raw::Profile;
use std::collections::BTreeMap;

/// Render the whole `.folded` document (lines sorted for determinism).
pub(crate) fn folded_text(profile: &Profile, model: &Model) -> String {
    let mut counts: BTreeMap<(usize, usize), u64> = BTreeMap::new();
    for sample in &profile.samples {
        *counts.entry((sample.thread, sample.stack)).or_insert(0) += 1;
    }
    let mut lines: Vec<String> = counts
        .iter()
        .map(|(&(thread, stack), &count)| folded_line(profile, model, thread, stack, count))
        .collect();
    lines.sort();
    if lines.is_empty() {
        return String::new();
    }
    lines.join("\n") + "\n"
}

/// `<fiberLabel>;root;...;leaf <count>` for one aggregated stack.
fn folded_line(
    profile: &Profile,
    model: &Model,
    thread: usize,
    stack: usize,
    count: u64,
) -> String {
    let label = profile
        .threads
        .get(thread)
        .map_or_else(|| format!("thread-{thread}"), |t| display_label(t, thread));
    let frames = model
        .root_first(stack)
        .into_iter()
        .filter_map(|id| model.frames.get(id))
        .map(|frame| sanitize(&frame.name));
    let path: Vec<String> = std::iter::once(sanitize(&label)).chain(frames).collect();
    format!("{} {count}", path.join(";"))
}

/// Frame names must not carry the collapsed format's separators: ';' and
/// whitespace become '_'.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c == ';' || c.is_whitespace() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbolize::SymFrame;
    use crate::testutil::{model_of, osp_frame, sample, thread};

    #[test]
    fn aggregates_all_samples_per_fiber_and_stack_root_first() {
        let (prof, model) = model_of(
            vec![thread(0, "main"), thread(3, "fiber")],
            vec![vec![100, 200], vec![300]],
            vec![
                sample(0, 0, 0, true),
                sample(1, 0, 0, false), // waiting samples count too
                sample(2, 1, 1, true),
            ],
            &[
                vec![osp_frame("fib", 5), osp_frame("main", 10)],
                vec![SymFrame::new("bad;name with space", "", 0)],
            ],
        );
        let text = folded_text(&prof, &model);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines, ["fiber-3;bad_name_with_space 1", "main;main;fib 2"]);
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn no_samples_produce_an_empty_document() {
        let (prof, model) = model_of(
            vec![thread(0, "main")],
            vec![vec![100]],
            vec![],
            &[vec![osp_frame("fib", 5)]],
        );
        assert_eq!(folded_text(&prof, &model), "");
    }
}
