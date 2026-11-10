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

/// `binding` holding `1` wrapped `depth` times in `open` and `close`.
pub(crate) fn nested_source(binding: &str, open: &str, close: &str, depth: usize) -> String {
    format!("{binding}{}1{}\n", open.repeat(depth), close.repeat(depth))
}
