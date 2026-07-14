//! V8 `.cpuprofile` export [PROF-CLI-RUN]: on-CPU samples of ALL fibers
//! merged into one time-ordered call tree. Times are MICROSECONDS and line
//! numbers 0-based, per the V8 contract; the file opens natively in VS
//! Code's built-in profile viewer.

use crate::model::Model;
use crate::raw::{Profile, Sample};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Nanoseconds per microsecond (raw times are ns, V8 wants µs).
const NS_PER_US: u64 = 1_000;

/// One call-tree node; ids are `index + 1` so the root is id 1.
#[derive(Debug)]
struct Node {
    /// Interned frame id; `None` only for the synthetic root.
    frame: Option<usize>,
    /// Samples whose leaf landed on this node.
    hit_count: u64,
    /// frame id → child node index.
    children: BTreeMap<usize, usize>,
}

impl Node {
    fn new(frame: Option<usize>) -> Self {
        Self {
            frame,
            hit_count: 0,
            children: BTreeMap::new(),
        }
    }
}

/// Build the complete `.cpuprofile` document. With zero on-CPU samples the
/// file is still valid: just the root node and empty sample arrays.
pub(crate) fn cpuprofile_json(profile: &Profile, model: &Model) -> Value {
    let samples = oncpu_sorted(profile);
    let (nodes, leaf_ids) = build_tree(model, &samples);
    json!({
        "nodes": nodes_json(model, &nodes),
        "startTime": 0,
        "endTime": samples.last().map_or(0, |s| s.t_ns / NS_PER_US),
        "samples": leaf_ids,
        "timeDeltas": time_deltas(&samples),
    })
}

/// On-CPU samples of every fiber, merged and ordered by time.
fn oncpu_sorted(profile: &Profile) -> Vec<Sample> {
    let mut samples: Vec<Sample> = profile
        .samples
        .iter()
        .copied()
        .filter(|s| s.on_cpu)
        .collect();
    samples.sort_by_key(|s| s.t_ns);
    samples
}

/// Insert every root-first stack path, recording each sample's leaf node id.
fn build_tree(model: &Model, samples: &[Sample]) -> (Vec<Node>, Vec<usize>) {
    let mut nodes = vec![Node::new(None)];
    let leaf_ids = samples
        .iter()
        .map(|sample| {
            let index = insert_path(&mut nodes, model, sample.stack);
            if let Some(node) = nodes.get_mut(index) {
                node.hit_count += 1;
            }
            index + 1
        })
        .collect();
    (nodes, leaf_ids)
}

/// Walk (and extend) the tree along one stack, returning the leaf node index.
fn insert_path(nodes: &mut Vec<Node>, model: &Model, stack: usize) -> usize {
    model
        .root_first(stack)
        .into_iter()
        .fold(0, |current, frame| child_of(nodes, current, frame))
}

/// The child of `parent` for `frame`, created on first sight.
fn child_of(nodes: &mut Vec<Node>, parent: usize, frame: usize) -> usize {
    if let Some(existing) = nodes
        .get(parent)
        .and_then(|n| n.children.get(&frame).copied())
    {
        return existing;
    }
    let index = nodes.len();
    nodes.push(Node::new(Some(frame)));
    if let Some(node) = nodes.get_mut(parent) {
        let _ = node.children.insert(frame, index);
    }
    index
}

/// Serialize the node table with 1-based ids.
fn nodes_json(model: &Model, nodes: &[Node]) -> Vec<Value> {
    nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            json!({
                "id": index + 1,
                "callFrame": call_frame(model, node.frame),
                "hitCount": node.hit_count,
                "children": node.children.values().map(|&child| child + 1).collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// V8 call frame: `url` is the file path and `lineNumber` is 0-based
/// (`line - 1`). Unknown lines (`line == 0`) become -1 — the same
/// convention the `(root)` node uses — never a fabricated line 1.
fn call_frame(model: &Model, frame: Option<usize>) -> Value {
    let Some(sym) = frame.and_then(|id| model.frames.get(id)) else {
        return json!({
            "functionName": "(root)", "scriptId": "0", "url": "",
            "lineNumber": -1, "columnNumber": -1,
        });
    };
    let line = i64::from(sym.line) - 1;
    let column = if line < 0 { -1 } else { 0 };
    json!({
        "functionName": sym.name, "scriptId": "0", "url": sym.file,
        "lineNumber": line, "columnNumber": column,
    })
}

/// µs deltas between consecutive samples; the first is measured from
/// `startTime` (0).
fn time_deltas(samples: &[Sample]) -> Vec<u64> {
    samples
        .iter()
        .scan(0_u64, |prev, sample| {
            let now = sample.t_ns / NS_PER_US;
            let delta = now.saturating_sub(*prev);
            *prev = now;
            Some(delta)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{model_of, osp_frame, sample, thread};

    fn fixture() -> Value {
        // Two on-CPU samples on [main->fib], one on [main->fib->fib]
        // (recursion), one waiting sample that must be excluded.
        let (prof, model) = model_of(
            vec![thread(0, "main")],
            vec![vec![100, 200], vec![100, 101, 200]],
            vec![
                sample(1_500_000, 0, 0, true),
                sample(500_000, 0, 0, true), // out of order on purpose
                sample(2_500_000, 0, 1, true),
                sample(3_500_000, 0, 0, false),
            ],
            &[
                vec![osp_frame("fib", 5), osp_frame("main", 10)],
                vec![
                    osp_frame("fib", 5),
                    osp_frame("fib", 5),
                    osp_frame("main", 10),
                ],
            ],
        );
        cpuprofile_json(&prof, &model)
    }

    fn node(doc: &Value, index: usize) -> &Value {
        doc.get("nodes")
            .and_then(Value::as_array)
            .unwrap()
            .get(index)
            .unwrap()
    }

    #[test]
    fn tree_shape_ids_and_recursion() {
        let doc = fixture();
        // root(1) -> main(2) -> fib(3) -> fib(4): recursion nests, ids are
        // 1-based, and the waiting sample contributed nothing.
        assert_eq!(doc.get("nodes").and_then(Value::as_array).unwrap().len(), 4);
        let root = node(&doc, 0);
        assert_eq!(root.get("id").and_then(Value::as_u64), Some(1));
        assert_eq!(
            root.pointer("/callFrame/functionName")
                .and_then(Value::as_str),
            Some("(root)")
        );
        assert_eq!(
            root.pointer("/callFrame/lineNumber")
                .and_then(Value::as_i64),
            Some(-1)
        );
        assert_eq!(root.get("children").unwrap(), &json!([2]));
        assert_eq!(node(&doc, 1).get("children").unwrap(), &json!([3]));
        assert_eq!(node(&doc, 2).get("children").unwrap(), &json!([4]));
    }

    #[test]
    fn call_frames_use_zero_based_lines() {
        let doc = fixture();
        let main = node(&doc, 1);
        assert_eq!(
            main.pointer("/callFrame/functionName")
                .and_then(Value::as_str),
            Some("main")
        );
        assert_eq!(
            main.pointer("/callFrame/url").and_then(Value::as_str),
            Some("/src/app.osp")
        );
        assert_eq!(
            main.pointer("/callFrame/lineNumber")
                .and_then(Value::as_u64),
            Some(9)
        );
        assert_eq!(
            main.pointer("/callFrame/scriptId").and_then(Value::as_str),
            Some("0")
        );
    }

    #[test]
    fn samples_are_time_ordered_leaf_ids_with_microsecond_deltas() {
        let doc = fixture();
        // Sorted by time: 500µs (fib, id 3), 1500µs (fib), 2500µs (fib fib, id 4).
        assert_eq!(doc.get("samples").unwrap(), &json!([3, 3, 4]));
        assert_eq!(doc.get("timeDeltas").unwrap(), &json!([500, 1000, 1000]));
        assert_eq!(doc.get("startTime").and_then(Value::as_u64), Some(0));
        assert_eq!(doc.get("endTime").and_then(Value::as_u64), Some(2500));
    }

    #[test]
    fn hit_counts_land_on_leaves() {
        let doc = fixture();
        assert_eq!(
            node(&doc, 2).get("hitCount").and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            node(&doc, 3).get("hitCount").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            node(&doc, 0).get("hitCount").and_then(Value::as_u64),
            Some(0)
        );
    }

    #[test]
    fn unknown_lines_export_as_minus_one_not_line_one() {
        // Runtime C frames symbolized without line info carry line 0; they
        // must not masquerade as source line 1.
        let (prof, model) = model_of(
            vec![thread(0, "main")],
            vec![vec![100]],
            vec![sample(0, 0, 0, true)],
            &[vec![crate::symbolize::SymFrame::new("tick", "", 0)]],
        );
        let doc = cpuprofile_json(&prof, &model);
        let tick = node(&doc, 1);
        assert_eq!(
            tick.pointer("/callFrame/lineNumber")
                .and_then(Value::as_i64),
            Some(-1)
        );
        assert_eq!(
            tick.pointer("/callFrame/columnNumber")
                .and_then(Value::as_i64),
            Some(-1)
        );
    }

    #[test]
    fn zero_oncpu_samples_still_emit_a_valid_file() {
        let (prof, model) = model_of(
            vec![thread(0, "main")],
            vec![vec![100]],
            vec![sample(0, 0, 0, false)],
            &[vec![osp_frame("fib", 5)]],
        );
        let doc = cpuprofile_json(&prof, &model);
        assert_eq!(doc.get("nodes").and_then(Value::as_array).unwrap().len(), 1);
        assert_eq!(doc.get("samples").unwrap(), &json!([]));
        assert_eq!(doc.get("timeDeltas").unwrap(), &json!([]));
        assert_eq!(doc.get("endTime").and_then(Value::as_u64), Some(0));
    }
}
