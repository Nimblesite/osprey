//! The structured documentation model. BOTH flavors' surface doc syntaxes —
//! `///` / `//!` in the Default flavor and `(** … *)` in the ML flavor — lower
//! into this one `DocComment`, so the type checker, the LSP, and the doc
//! exporter never see a flavor difference ([FLAVOR-BOUNDARY]). Implements
//! spec 0026 `[DOC-MODEL]`.
//!
//! The body markup (`CommonMark` + `[Symbol]` links + ```osprey``` doctests) is
//! identical across flavors; parsing raw text into this structure lives in
//! `osprey-syntax` (flavor-neutral), so this crate stays a pure data model.

/// Whether a doc comment documents the declaration that follows it (`Outer`)
/// or the enclosing module/file (`Inner` — Default `//!`). Implements
/// [DOC-ATTACH].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocScope {
    /// Documents the following declaration (`///`, `(** *)`).
    Outer,
    /// Documents the enclosing module/file (`//!`).
    Inner,
}

/// One executable example extracted from a doc comment's ```osprey``` fence.
/// Implements [DOC-DOCTEST-HARNESS].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocExample {
    /// The Osprey snippet, compiled under the enclosing file's flavor.
    pub code: String,
    /// Expected stdout for the golden harness, when an `output` fence follows.
    pub expected_output: Option<String>,
    /// Compile-only (no run) for type-level examples (no `output` fence).
    pub run: bool,
}

/// A structured documentation comment. Implements [DOC-MODEL]. Recognised
/// sections lower into the typed fields; everything else stays in `body`.
/// `[Symbol]` links in `summary`/`body` are left unresolved here — the LSP
/// resolves them against the symbol table at hover/definition time
/// ([DOC-LINK]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocComment {
    /// First paragraph — the one-line summary.
    pub summary: String,
    /// Full Markdown body EXCLUDING the recognised sections. Empty when the
    /// doc is summary-only.
    pub body: String,
    /// `# Parameters` / `@param` — name → description, in author order.
    pub params: Vec<(String, String)>,
    /// `# Returns` / `@return`.
    pub returns: Option<String>,
    /// `# Raises` / `# Errors` / `@raise` — effect or error name → description.
    pub raises: Vec<(String, String)>,
    /// `# Examples` — extracted ```osprey``` fences with optional output.
    pub examples: Vec<DocExample>,
    /// `# See also` / `@see` — `[Symbol]` references and external links.
    pub see_also: Vec<String>,
    /// `# Since` / `@since` — version introduced.
    pub since: Option<String>,
    /// `# Deprecated` / `@deprecated` — present ⇒ deprecated; the reason.
    pub deprecated: Option<String>,
    /// `//!` module authorship, optional.
    pub author: Option<String>,
    /// Outer (declaration) vs inner (module) scope.
    pub scope: DocScope,
}

impl DocComment {
    /// Create a doc with its free-form text populated and every optional
    /// section empty. Flavor parsers fill the recognised sections afterwards.
    #[must_use]
    pub fn new(summary: impl Into<String>, body: impl Into<String>, scope: DocScope) -> DocComment {
        DocComment {
            summary: summary.into(),
            body: body.into(),
            params: Vec::new(),
            returns: None,
            raises: Vec::new(),
            examples: Vec::new(),
            see_also: Vec::new(),
            since: None,
            deprecated: None,
            author: None,
            scope,
        }
    }

    /// A summary-only outer doc — the common case, and the shape a bare doc
    /// comment with no recognised sections lowers to.
    #[must_use]
    pub fn summary_only(summary: impl Into<String>) -> DocComment {
        Self::new(summary, String::new(), DocScope::Outer)
    }

    /// Render the whole doc comment as the Markdown block a hover shows: the
    /// summary, the body, then each populated section as a heading. `[Symbol]`
    /// links are preserved verbatim so the LSP client renders them as links.
    /// Implements [DOC-EXPORT] (the hover half).
    #[must_use]
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        push_para(&mut out, &self.summary);
        push_para(&mut out, &self.body);
        if let Some(reason) = &self.deprecated {
            push_para(&mut out, &format!("**Deprecated.** {reason}"));
        }
        render_pairs(&mut out, "Parameters", &self.params);
        if let Some(r) = &self.returns {
            push_section(&mut out, "Returns", r);
        }
        render_pairs(&mut out, "Raises", &self.raises);
        render_examples(&mut out, &self.examples);
        if !self.see_also.is_empty() {
            push_section(&mut out, "See also", &self.see_also.join(", "));
        }
        if let Some(s) = &self.since {
            push_section(&mut out, "Since", s);
        }
        out.trim_end().to_string()
    }
}

/// Append `text` as a paragraph (blank-line separated) when non-empty.
fn push_para(out: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(text);
}

/// Append a `**Heading**` followed by prose.
fn push_section(out: &mut String, heading: &str, text: &str) {
    push_para(out, &format!("**{heading}**"));
    push_para(out, text);
}

/// Append a `**Heading**` then a `- name — text` bullet per pair.
fn render_pairs(out: &mut String, heading: &str, pairs: &[(String, String)]) {
    if pairs.is_empty() {
        return;
    }
    push_para(out, &format!("**{heading}**"));
    let list = pairs
        .iter()
        .map(|(name, text)| format!("- `{name}` — {text}"))
        .collect::<Vec<_>>()
        .join("\n");
    push_para(out, &list);
}

/// Append the `**Examples**` section: each snippet in an ```osprey``` fence.
fn render_examples(out: &mut String, examples: &[DocExample]) {
    if examples.is_empty() {
        return;
    }
    push_para(out, "**Examples**");
    for ex in examples {
        push_para(out, &format!("```osprey\n{}\n```", ex.code));
        if let Some(o) = &ex.expected_output {
            push_para(out, &format!("```\n{o}\n```"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_only_leaves_every_section_empty() {
        let doc = DocComment::summary_only("Adds two ints.");
        assert_eq!(doc.summary, "Adds two ints.");
        assert!(doc.body.is_empty());
        assert!(doc.params.is_empty() && doc.returns.is_none());
        assert_eq!(doc.scope, DocScope::Outer);
        // A summary-only doc renders as just its summary line.
        assert_eq!(doc.render_markdown(), "Adds two ints.");
    }

    #[test]
    fn render_markdown_emits_every_populated_section_in_order() {
        let mut doc = DocComment::new("Divide.", "Long form.", DocScope::Outer);
        doc.deprecated = Some("use `div` instead".to_owned());
        doc.params = vec![("a".to_owned(), "numerator".to_owned())];
        doc.returns = Some("the quotient".to_owned());
        doc.raises = vec![("DivByZero".to_owned(), "when `b == 0`".to_owned())];
        doc.examples = vec![DocExample {
            code: "divide(6, 2)".to_owned(),
            expected_output: Some("3".to_owned()),
            run: true,
        }];
        doc.see_also = vec!["[div]".to_owned()];
        doc.since = Some("1.2".to_owned());
        let md = doc.render_markdown();
        // Summary and body head the block; each section carries its bold heading.
        assert!(md.starts_with("Divide.\n\nLong form."));
        for needle in [
            "**Deprecated.** use `div` instead",
            "**Parameters**",
            "- `a` — numerator",
            "**Returns**",
            "the quotient",
            "**Raises**",
            "- `DivByZero` — when `b == 0`",
            "**Examples**",
            "```osprey\ndivide(6, 2)\n```",
            "**See also**",
            "[div]",
            "**Since**",
            "1.2",
        ] {
            assert!(md.contains(needle), "missing {needle:?} in:\n{md}");
        }
        // The trailing whitespace is trimmed.
        assert_eq!(md, md.trim_end());
    }
}
