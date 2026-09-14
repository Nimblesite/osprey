//! The flavor-neutral documentation-comment body parser. Both flavors strip
//! their sigil (`///` in Default, `(** … *)` in ML) down to raw doc text, then
//! call [`parse_doc`] to lower that text into the shared
//! [`osprey_ast::DocComment`]. One parser, both flavors — the body markup is
//! identical past the sigil ([FLAVOR-BOUNDARY]). Implements spec 0026
//! `[DOC-BODY-MARKDOWN]`, `[DOC-SECTIONS]`, `[DOC-DOCTEST-HARNESS]`.

use osprey_ast::{DocComment, DocExample, DocScope};

/// Lower raw doc text (sigil already stripped) into a structured
/// [`DocComment`]. `scope` records whether the source sigil was outer or inner.
#[must_use]
pub(crate) fn parse_doc(raw: &str, scope: DocScope) -> DocComment {
    let (free, sections) = split_sections(raw);
    let (summary, body) = split_summary(&free);
    let mut doc = DocComment::new(summary, body, scope);
    for (heading, content) in sections {
        apply_section(&mut doc, &heading, &content);
    }
    apply_inline_tags(&mut doc);
    doc
}

/// Split the text into the free-form prefix (before the first recognised
/// heading) and a list of `(heading, content)` sections. A recognised heading
/// is a line `# Word` whose word is one of the known section names; any other
/// `#` line stays in the preceding section's content as ordinary Markdown.
fn split_sections(raw: &str) -> (String, Vec<(String, String)>) {
    let mut free = String::new();
    let mut sections: Vec<(String, String)> = Vec::new();
    for line in raw.lines() {
        match recognised_heading(line) {
            Some(h) => sections.push((h, String::new())),
            None => match sections.last_mut() {
                Some((_, content)) => push_line(content, line),
                None => push_line(&mut free, line),
            },
        }
    }
    (free.trim().to_string(), sections)
}

/// The canonical section name a `# Heading` line names, if it is one we lower
/// into a typed field. Case-insensitive; `Errors` is an alias of `Raises`.
fn recognised_heading(line: &str) -> Option<String> {
    let t = line.trim();
    let name = t.strip_prefix('#')?.trim().to_lowercase();
    match name.as_str() {
        "parameters" | "params" => Some("parameters".to_string()),
        "returns" | "return" => Some("returns".to_string()),
        "raises" | "errors" => Some("raises".to_string()),
        "examples" | "example" => Some("examples".to_string()),
        "see also" | "see" => Some("see also".to_string()),
        "since" => Some("since".to_string()),
        "deprecated" => Some("deprecated".to_string()),
        _ => None,
    }
}

/// What either flavor reports for a `//!` that sits where no scope can hold it.
/// `outer` is that flavor's own spelling of a declaration doc — `///` or
/// `(** … *)` — which is what the author most likely meant ([DOC-SIGIL-INNER]).
pub(crate) fn misplaced_inner_doc(outer: &str) -> String {
    format!(
        "`//!` documents the enclosing file, namespace or module; write it as the \
         first item of one, or use `{outer}` to document the declaration that follows"
    )
}

/// Lower one recognised section's content into the matching field.
fn apply_section(doc: &mut DocComment, heading: &str, content: &str) {
    let trimmed = content.trim();
    match heading {
        "parameters" => doc.params.extend(parse_bullets(content)),
        "returns" => doc.returns = non_empty(trimmed),
        "raises" => doc.raises.extend(parse_bullets(content)),
        "examples" => {
            let (examples, problems) = parse_examples(content);
            doc.examples.extend(examples);
            doc.example_problems.extend(problems);
        }
        "see also" => doc.see_also.extend(parse_see_also(trimmed)),
        "since" => doc.since = non_empty(trimmed),
        "deprecated" => doc.deprecated = non_empty(trimmed).or(Some(String::new())),
        _ => {}
    }
}

/// Parse `- name: description` bullets (Parameters / Raises). A bullet with no
/// colon is treated as `name` with an empty description.
fn parse_bullets(content: &str) -> Vec<(String, String)> {
    content
        .lines()
        .filter_map(|l| l.trim().strip_prefix('-').map(str::trim))
        .filter(|l| !l.is_empty())
        .map(|l| match l.split_once(':') {
            Some((name, desc)) => (name.trim().to_string(), desc.trim().to_string()),
            None => (l.to_string(), String::new()),
        })
        .collect()
}

/// The labels an example fence may carry. The spec spells an ML snippet
/// ```` ```osprey-ml ````, and ML authors copy that, or the `ospml` extension.
/// The label never selects the flavor: an example compiles in the flavor of the
/// file that documents it.
const EXAMPLE_LABELS: [&str; 3] = ["osprey", "osprey-ml", "ospml"];

/// What an `output` fence with no example before it is reported as.
const ORPHANED_OUTPUT: &str = "an `output` fence follows no example, so its expected output \
    is never compared; put it directly after the ```osprey fence it belongs to";

/// Extract the example fences, each optionally followed by an ```output```
/// fence, into [`DocExample`]s. Implements [DOC-DOCTEST-HARNESS].
///
/// Every fence's body is consumed whole, whatever its label, so a line inside
/// one is never mistaken for the opener of another. An `output` fence reached
/// on its own belongs to no example: it is returned as a problem rather than
/// dropped, because dropping it is how a wrong expectation used to pass.
fn parse_examples(content: &str) -> (Vec<DocExample>, Vec<String>) {
    let mut examples = Vec::new();
    let mut problems = Vec::new();
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        match fence_label(line) {
            Some(label) if EXAMPLE_LABELS.contains(&label) => examples.push(example(&mut lines)),
            Some(label) => {
                let _ = collect_fence(&mut lines);
                if label == "output" {
                    problems.push(ORPHANED_OUTPUT.to_owned());
                }
            }
            None => {}
        }
    }
    (examples, problems)
}

/// One example whose opening fence was just read, with the output fence that
/// belongs to it.
fn example<'a, I: Iterator<Item = &'a str>>(lines: &mut std::iter::Peekable<I>) -> DocExample {
    let code = collect_fence(lines);
    let expected_output = output_after(lines);
    DocExample {
        run: expected_output.is_some(),
        code,
        expected_output,
    }
}

/// The `output` fence directly after an example. Blank lines may separate the
/// two — that is ordinary Markdown — but nothing else may: prose in between
/// leaves the example compile-only and the fence an orphan the caller reports.
fn output_after<'a, I: Iterator<Item = &'a str>>(
    lines: &mut std::iter::Peekable<I>,
) -> Option<String> {
    while lines.peek().is_some_and(|line| line.trim().is_empty()) {
        let _ = lines.next();
    }
    if lines.peek().and_then(|line| fence_label(line)) != Some("output") {
        return None;
    }
    let _ = lines.next();
    Some(collect_fence(lines))
}

/// The info string of a line that opens a fenced block, tolerant of leading
/// indentation; `None` for any other line.
fn fence_label(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("```").map(str::trim)
}

/// Collect fenced-block lines until the closing fence; the iterator is left
/// just past that fence.
fn collect_fence<'a, I: Iterator<Item = &'a str>>(lines: &mut std::iter::Peekable<I>) -> String {
    let mut body: Vec<&str> = Vec::new();
    for l in lines.by_ref() {
        if l.trim_start().starts_with("```") {
            break;
        }
        body.push(l);
    }
    body.join("\n")
}

/// Parse a `# See also` body: `[Symbol]` links and bare URLs, comma- or
/// line-separated.
fn parse_see_also(content: &str) -> Vec<String> {
    content
        .split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Recognise the ocamldoc/Javadoc `@param`/`@return`/… tags that appear inline
/// in the free body (an ML muscle-memory alias for the heading sections) and
/// move them into typed fields. Implements the [DOC-SECTIONS] tag aliases.
fn apply_inline_tags(doc: &mut DocComment) {
    let mut kept = Vec::new();
    for line in doc.body.clone().lines() {
        if !take_tag_line(doc, line.trim()) {
            kept.push(line.to_string());
        }
    }
    doc.body = kept.join("\n").trim().to_string();
}

/// If `line` is an `@tag …` alias, fold it into the matching field and return
/// true (drop it from the body); otherwise return false (keep it).
fn take_tag_line(doc: &mut DocComment, line: &str) -> bool {
    let Some(rest) = line.strip_prefix('@') else {
        return false;
    };
    let (tag, arg) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let arg = arg.trim();
    match tag {
        "param" => {
            if let Some((name, desc)) = arg.split_once(char::is_whitespace) {
                doc.params
                    .push((name.trim().to_string(), desc.trim().to_string()));
            }
            true
        }
        "return" | "returns" => {
            doc.returns = non_empty(arg);
            true
        }
        "raise" | "raises" | "throws" => {
            if let Some((name, desc)) = arg.split_once(char::is_whitespace) {
                doc.raises
                    .push((name.trim().to_string(), desc.trim().to_string()));
            } else if !arg.is_empty() {
                doc.raises.push((arg.to_string(), String::new()));
            }
            true
        }
        "see" => {
            doc.see_also.push(arg.to_string());
            true
        }
        "since" => {
            doc.since = non_empty(arg);
            true
        }
        "deprecated" => {
            doc.deprecated = Some(arg.to_string());
            true
        }
        "author" => {
            doc.author = non_empty(arg);
            true
        }
        _ => false,
    }
}

/// The summary is the first paragraph; the rest is the body.
fn split_summary(free: &str) -> (String, String) {
    match free.split_once("\n\n") {
        Some((head, tail)) => (normalise_para(head), tail.trim().to_string()),
        None => (normalise_para(free), String::new()),
    }
}

/// Collapse a paragraph's internal newlines to spaces (a summary is one line).
fn normalise_para(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn push_line(buf: &mut String, line: &str) {
    buf.push_str(line);
    buf.push('\n');
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Find every `[Symbol]` intra-doc link in a rendered doc string — the spans
/// the LSP turns into hoverable/clickable references ([DOC-LINK]). A link is
/// `[ident]` or `[Ident.op]`; `[]` and links containing spaces or markdown
/// link syntax (`](`) are ignored.
#[must_use]
pub fn doc_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Walk each `[` and pair it with the next `]`; a UTF-8-safe scan via
    // `char_indices` (no byte indexing, so multi-byte prose can't panic).
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(']') else {
            break;
        };
        let inner = &after_open[..close];
        let after = after_open[close + 1..].chars().next();
        if is_symbol_link(inner) && after != Some('(') {
            out.push(inner.to_string());
        }
        rest = &after_open[close + 1..];
    }
    out
}

/// A `[Symbol]` link body: a dotted identifier path, no spaces.
fn is_symbol_link(inner: &str) -> bool {
    !inner.is_empty()
        && !inner.contains(char::is_whitespace)
        && inner
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && inner.chars().next().is_some_and(char::is_alphabetic)
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
    use super::*;

    #[test]
    fn summary_and_body_split_on_blank_line() {
        // [DOC-BODY-MARKDOWN] both flavors share this summary/body split.
        let d = parse_doc(
            "Doubles its argument.\n\nA longer note here.",
            DocScope::Outer,
        );
        assert_eq!(d.summary, "Doubles its argument.");
        assert_eq!(d.body, "A longer note here.");
    }

    #[test]
    fn recognised_sections_lower_into_fields() {
        // [DOC-SECTIONS] recognised headings populate the structured fields.
        let raw = "Divides two numbers.\n\n\
                   # Parameters\n\
                   - numerator: the top\n\
                   - denominator: the bottom\n\n\
                   # Returns\n\
                   the quotient\n\n\
                   # Raises\n\
                   - DivByZero: on zero\n\n\
                   # Since\n0.4.0";
        let d = parse_doc(raw, DocScope::Outer);
        assert_eq!(d.summary, "Divides two numbers.");
        assert_eq!(
            d.params,
            vec![
                ("numerator".to_string(), "the top".to_string()),
                ("denominator".to_string(), "the bottom".to_string())
            ]
        );
        assert_eq!(d.returns.as_deref(), Some("the quotient"));
        assert_eq!(
            d.raises,
            vec![("DivByZero".to_string(), "on zero".to_string())]
        );
        assert_eq!(d.since.as_deref(), Some("0.4.0"));
    }

    #[test]
    fn at_tag_aliases_fold_into_fields() {
        // [DOC-SECTIONS] tag aliases lower to the same fields as headings.
        let raw = "Summary.\n\n@param x the input\n@return the output\n@since 1.0";
        let d = parse_doc(raw, DocScope::Outer);
        assert_eq!(d.params, vec![("x".to_string(), "the input".to_string())]);
        assert_eq!(d.returns.as_deref(), Some("the output"));
        assert_eq!(d.since.as_deref(), Some("1.0"));
        assert!(d.body.is_empty(), "tag lines are removed from the body");
    }

    #[test]
    fn doctest_fences_extract_with_expected_output() {
        // [DOC-DOCTEST-HARNESS] the model retains code, output, and run mode.
        let raw = "Doubles.\n\n# Examples\n```osprey\nprint(double(21))\n```\n```output\n42\n```";
        let d = parse_doc(raw, DocScope::Outer);
        assert_eq!(d.examples.len(), 1);
        assert_eq!(d.examples[0].code, "print(double(21))");
        assert_eq!(d.examples[0].expected_output.as_deref(), Some("42"));
        assert!(d.examples[0].run);
    }

    #[test]
    fn ml_labelled_fences_are_examples_like_osprey_ones() {
        // [DOC-DOCTEST-HARNESS] the spec spells an ML snippet ```osprey-ml, so
        // that label and the `ospml` extension both open an example.
        for label in ["osprey-ml", "ospml"] {
            let raw = format!(
                "Doubles.\n\n# Examples\n```{label}\nprint (double 2)\n```\n```output\n4\n```"
            );
            let d = parse_doc(&raw, DocScope::Outer);
            assert_eq!(d.examples.len(), 1, "{label}");
            assert_eq!(
                d.examples[0].expected_output.as_deref(),
                Some("4"),
                "{label}"
            );
            assert!(d.examples[0].run, "{label}");
            assert!(d.example_problems.is_empty(), "{label}");
        }
    }

    #[test]
    fn blank_lines_may_separate_an_example_from_its_output() {
        // [DOC-DOCTEST-HARNESS] a blank line before ```output is ordinary
        // Markdown; it must not quietly make the example compile-only.
        let raw = "Adds.\n\n# Examples\n```osprey\nprint(add(1, 2))\n```\n\n\n```output\n3\n```";
        let d = parse_doc(raw, DocScope::Outer);
        assert_eq!(d.examples.len(), 1);
        assert_eq!(d.examples[0].expected_output.as_deref(), Some("3"));
        assert!(d.examples[0].run);
        assert!(d.example_problems.is_empty());
    }

    #[test]
    fn an_output_fence_no_example_precedes_is_reported_not_dropped() {
        // [DOC-DOCTEST-HARNESS] an unlabelled fence, or prose, before
        // ```output leaves that output belonging to nothing. Dropping it is how
        // a wrong expectation passed the gate.
        for raw in [
            "Adds.\n\n# Examples\n```\nprint(add(1, 2))\n```\n```output\n3\n```",
            "Adds.\n\n# Examples\n```osprey\nprint(add(1, 2))\n```\nIt prints:\n```output\n3\n```",
        ] {
            let d = parse_doc(raw, DocScope::Outer);
            assert_eq!(d.example_problems.len(), 1, "{raw}");
            assert!(d.example_problems[0].contains("`output` fence"), "{raw}");
            assert!(d.examples.iter().all(|example| !example.run), "{raw}");
        }
    }

    #[test]
    fn symbol_links_are_found_and_markdown_links_ignored() {
        // [DOC-LINK] bare/dotted symbols are links; ordinary Markdown is not.
        let links = doc_links("See [safeDivide] and [Console.emit], not [text](http://x).");
        assert_eq!(
            links,
            vec!["safeDivide".to_string(), "Console.emit".to_string()]
        );
    }

    #[test]
    fn unclosed_bracket_stops_the_link_scan_without_panicking() {
        // A `[` with no matching `]` ends the scan; earlier links still surface.
        assert_eq!(doc_links("[ok] then [dangling"), vec!["ok".to_string()]);
        assert!(doc_links("[]").is_empty(), "empty brackets are not links");
    }

    #[test]
    fn deprecated_and_see_also_sections_and_singular_aliases_lower() {
        // `# Example`/`# See`/`# Deprecated` aliases, a bullet without a colon,
        // and a doctest with no `output` fence (compile-only, run == false).
        let raw = "Legacy op.\n\n\
                   # Parameters\n- bareName\n\n\
                   # See also\n[newOp], https://x\n\n\
                   # Example\n```osprey\nlegacy()\n```\n\n\
                   # Deprecated\nuse `newOp`";
        let d = parse_doc(raw, DocScope::Outer);
        assert_eq!(d.params, vec![("bareName".to_string(), String::new())]);
        assert_eq!(
            d.see_also,
            vec!["[newOp]".to_string(), "https://x".to_string()]
        );
        assert_eq!(d.examples.len(), 1);
        assert!(!d.examples[0].run, "no output fence ⇒ compile-only");
        assert_eq!(d.deprecated.as_deref(), Some("use `newOp`"));
    }

    #[test]
    fn all_at_tag_aliases_fold_into_their_fields() {
        let raw = "Summary.\n\n\
                   @raise DivByZero on zero\n\
                   @throws Overflow\n\
                   @see [other]\n\
                   @deprecated gone in 2.0\n\
                   @author Devon";
        let d = parse_doc(raw, DocScope::Outer);
        assert_eq!(
            d.raises,
            vec![
                ("DivByZero".to_string(), "on zero".to_string()),
                ("Overflow".to_string(), String::new()),
            ]
        );
        assert_eq!(d.see_also, vec!["[other]".to_string()]);
        assert_eq!(d.deprecated.as_deref(), Some("gone in 2.0"));
        assert_eq!(d.author.as_deref(), Some("Devon"));
    }
}
