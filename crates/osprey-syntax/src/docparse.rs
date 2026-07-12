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

/// Lower one recognised section's content into the matching field.
fn apply_section(doc: &mut DocComment, heading: &str, content: &str) {
    let trimmed = content.trim();
    match heading {
        "parameters" => doc.params.extend(parse_bullets(content)),
        "returns" => doc.returns = non_empty(trimmed),
        "raises" => doc.raises.extend(parse_bullets(content)),
        "examples" => doc.examples.extend(parse_examples(content)),
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

/// Extract ```osprey``` fenced blocks, each optionally followed by an
/// ```output``` fence, into [`DocExample`]s. Implements [DOC-DOCTEST-HARNESS].
fn parse_examples(content: &str) -> Vec<DocExample> {
    let mut out = Vec::new();
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        if !is_fence(line, "osprey") {
            continue;
        }
        let code = collect_fence(&mut lines);
        let expected_output = if lines.peek().is_some_and(|l| is_fence(l, "output")) {
            let _ = lines.next();
            Some(collect_fence(&mut lines))
        } else {
            None
        };
        let run = expected_output.is_some();
        out.push(DocExample {
            code,
            expected_output,
            run,
        });
    }
    out
}

/// True when `line` opens a fenced block with the given info string (```osprey
/// / ```output), tolerant of leading indentation.
fn is_fence(line: &str, info: &str) -> bool {
    let t = line.trim_start();
    t.strip_prefix("```")
        .is_some_and(|rest| rest.trim() == info)
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
        let d = parse_doc(
            "Doubles its argument.\n\nA longer note here.",
            DocScope::Outer,
        );
        assert_eq!(d.summary, "Doubles its argument.");
        assert_eq!(d.body, "A longer note here.");
    }

    #[test]
    fn recognised_sections_lower_into_fields() {
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
        let raw = "Summary.\n\n@param x the input\n@return the output\n@since 1.0";
        let d = parse_doc(raw, DocScope::Outer);
        assert_eq!(d.params, vec![("x".to_string(), "the input".to_string())]);
        assert_eq!(d.returns.as_deref(), Some("the output"));
        assert_eq!(d.since.as_deref(), Some("1.0"));
        assert!(d.body.is_empty(), "tag lines are removed from the body");
    }

    #[test]
    fn doctest_fences_extract_with_expected_output() {
        let raw = "Doubles.\n\n# Examples\n```osprey\nprint(double(21))\n```\n```output\n42\n```";
        let d = parse_doc(raw, DocScope::Outer);
        assert_eq!(d.examples.len(), 1);
        assert_eq!(d.examples[0].code, "print(double(21))");
        assert_eq!(d.examples[0].expected_output.as_deref(), Some("42"));
        assert!(d.examples[0].run);
    }

    #[test]
    fn symbol_links_are_found_and_markdown_links_ignored() {
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
