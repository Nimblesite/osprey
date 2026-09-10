//! Page-level documentation semantics. Implements [DOC-EXPORT-HTML].

use super::{article, fresh_dir, generate, page, read};

#[test]
fn a_summary_already_in_markdown_is_shown_once_inside_the_article() {
    let dir = fresh_dir("summary_once");
    let mut documented = page(
        "api/example",
        "example",
        "Function",
        "# example\n\nThe summary.\n",
    );
    documented.summary = "The summary.".into();
    generate(&dir, &[documented], "osprey", &[]).expect("site");
    let body = article(&read(&dir, "api/example.html"));
    assert_eq!(body.matches("The summary.").count(), 1, "{body}");
}
