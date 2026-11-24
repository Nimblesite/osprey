//! The deepest nesting each flavour accepts (#362), shared so the CLI and the
//! language server prove the same boundary [FAILURE-EXPLICIT].

/// `(extension, binding, open, close, deepest)`. ML counts delimiters; Default
/// counts CST nodes, so a list level costs it more than a group level.
pub(crate) const DEPTH_BOUNDARIES: [(&str, &str, &str, &str, usize); 4] = [
    ("ospml", "value = ", "(", ")", 512),
    ("ospml", "value = ", "[", "]", 512),
    ("osp", "let value = ", "(", ")", 253),
    ("osp", "let value = ", "[", "]", 126),
];

/// Closing brackets per line: eight or more on one line is a `closing-run`
/// warning, and the deepest accepted source must be free of diagnostics.
const CLOSERS_PER_LINE: usize = 7;

/// `binding` holding `1` wrapped `depth` times in `open` and `close`, the
/// closers spread over indented continuation lines.
pub(crate) fn nested_source(binding: &str, open: &str, close: &str, depth: usize) -> String {
    let closers: Vec<String> = (0..depth)
        .step_by(CLOSERS_PER_LINE)
        .map(|start| close.repeat(CLOSERS_PER_LINE.min(depth - start)))
        .collect();
    format!("{binding}{}1{}\n", open.repeat(depth), closers.join("\n  "))
}
