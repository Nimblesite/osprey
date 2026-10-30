use super::parse_script;

/// Blank lines and comments do not become replay steps.
#[test]
fn skips_blank_and_comment_lines() {
    let steps = parse_script("\n// comment\n  // indented\n {\"send\":null,\"expect\":{}}\n");
    assert_eq!(steps.as_ref().map(Vec::len), Ok(1));
    assert!(steps.is_ok_and(|steps| steps[0].send.is_null()));
}

/// A malformed JSON line reports its original one-based line number.
#[test]
fn malformed_line_names_its_number() {
    let problem = parse_script("\n// comment\n{not json}\n");
    assert!(problem.is_err_and(|problem| problem.starts_with("step 3:")));
}
