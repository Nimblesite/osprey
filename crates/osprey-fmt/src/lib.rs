//! The Osprey source formatter.
//!
//! One entry point, [`format_source`], formats a whole file in the flavor it is
//! told. The two flavors lay code out very differently — the Default flavor is
//! C-style and brace-driven ([`brace`]); the ML flavor is indentation-significant
//! ([`layout`]) — but both share the flavor-neutral line [`scan`]ner and obey the
//! same two guarantees:
//!
//! * **Meaning-preserving.** After reformatting, the candidate text is reparsed
//!   and its AST is compared to the original's. If they differ in any way (or the
//!   candidate fails to parse), the file is left unchanged and the rejection is
//!   **reported** — it is the formatter's own defect, so it is never swallowed
//!   into a result indistinguishable from "already formatted". ML input also has
//!   one constrained recovery for an accidentally dedented suffix: it is accepted
//!   only when indenting from a reported error restores a complete parse.
//! * **Idempotent.** Formatting already-formatted text is a no-op.
//!
//! The same function backs both the `osprey fmt` CLI command and the language
//! server's `textDocument/formatting` request, so an editor and the command line
//! always agree.

mod brace;
mod layout;
mod scan;

pub use osprey_syntax::Flavor;

/// Columns per indentation level. Both flavors render one nesting level as this
/// many spaces.
const INDENT_WIDTH: usize = 4;

/// Format `src` in the given [`Flavor`].
///
/// # Errors
/// Returns the source's syntax errors (as `line:col: message` strings) when the
/// input does not parse, except for a recoverable dedented ML suffix; or
/// [`DECLINED`] when the formatter's own output fails the meaning-preservation
/// guard in [`accept`].
pub fn format_source(src: &str, flavor: Flavor) -> Result<String, Vec<String>> {
    let mut source = src.to_owned();
    let mut parsed = osprey_syntax::parse_program_with_flavor(&source, flavor);
    if !parsed.errors.is_empty() && flavor == Flavor::Ml {
        if let Some(repaired) = layout::repair_dedented_suffix(&source, &parsed.errors) {
            source = repaired;
            parsed = osprey_syntax::parse_program_with_flavor(&source, flavor);
        }
    }
    if !parsed.errors.is_empty() {
        return Err(parsed.errors.iter().map(error_line).collect());
    }
    let candidate = match flavor {
        Flavor::Default => brace::format(&source),
        Flavor::Ml => layout::format(&source),
    };
    accept(&parsed.program, candidate, flavor)
}

/// Format `src` using the flavor resolved from `path` (its extension and any
/// in-source flavor marker), the same precedence the compiler uses.
///
/// # Errors
/// Returns a single-element error list when the flavor cannot be resolved (a
/// marker/extension conflict), otherwise the errors from [`format_source`].
pub fn format_for_path(path: &str, src: &str) -> Result<String, Vec<String>> {
    match osprey_syntax::resolve_flavor(None, path, src) {
        Ok(flavor) => format_source(src, flavor),
        Err(message) => Err(vec![message]),
    }
}

/// Reported when the reparse guard rejects the formatter's own output.
///
/// The wording says whose fault it is, because it is never the author's: the
/// input parsed cleanly a moment earlier.
pub const DECLINED: &str =
    "formatter declined: its own output does not reparse to the same program (formatter defect); \
     the file is left unchanged";

/// The guard that makes formatting meaning-preserving: `candidate` is returned
/// only when it reparses to exactly the same program as the original.
///
/// A rejection is **reported**, not swallowed. Returning the input verbatim
/// instead — as this did until plan 0019 closed it — is indistinguishable from
/// "this file was already formatted", so a formatter that had stopped
/// formatting a whole class of file would sail through the corpus audit and
/// through `osprey fmt --check` alike.
///
/// # Errors
/// Returns [`DECLINED`], followed by the candidate's own syntax errors when it
/// no longer parses at all.
fn accept(
    original: &osprey_ast::Program,
    candidate: String,
    flavor: Flavor,
) -> Result<String, Vec<String>> {
    let reparsed = osprey_syntax::parse_program_with_flavor(&candidate, flavor);
    if !reparsed.errors.is_empty() {
        return Err(std::iter::once(DECLINED.to_owned())
            .chain(reparsed.errors.iter().map(error_line))
            .collect());
    }
    // Compared without source coordinates: moving a node to a new line is the
    // whole point of formatting, so a position difference is the one difference
    // that must not count ([`osprey_ast::canonical`]).
    if osprey_ast::canonical::without_positions(&reparsed.program)
        == osprey_ast::canonical::without_positions(original)
    {
        return Ok(candidate);
    }
    Err(vec![DECLINED.to_owned()])
}

/// Render a syntax error as `line:col: message`.
fn error_line(err: &osprey_syntax::SyntaxError) -> String {
    format!(
        "{}:{}: {}",
        err.position.line, err.position.column, err.message
    )
}

/// The leading whitespace for a given nesting depth.
pub(crate) fn indent_to(depth: i32) -> String {
    let levels = usize::try_from(depth.max(0)).unwrap_or(0);
    " ".repeat(levels * INDENT_WIDTH)
}

/// Join formatted lines into final output: collapse runs of blank lines to a
/// single separator, drop leading and trailing blanks, and end with exactly one
/// newline. Empty input yields an empty string.
pub(crate) fn finalize(lines: &[String]) -> String {
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut pending_blank = false;
    for line in lines {
        if line.is_empty() {
            pending_blank = true;
            continue;
        }
        if pending_blank && !out.is_empty() {
            out.push("");
        }
        pending_blank = false;
        out.push(line);
    }
    if out.is_empty() {
        return String::new();
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_flavor_reindents_and_round_trips() {
        let src = "fn main() = {\nprint(1)\n}\n";
        let out = format_source(src, Flavor::Default).expect("formats");
        assert_eq!(out, "fn main() = {\n    print(1)\n}\n");
        // Idempotent.
        assert_eq!(
            format_source(&out, Flavor::Default).expect("re-formats"),
            out
        );
    }

    #[test]
    fn ml_flavor_regrids_layout() {
        let src = "main () =\n  print 1\n";
        let out = format_source(src, Flavor::Ml).expect("formats");
        assert_eq!(out, "main () =\n    print 1\n");
    }

    #[test]
    fn ml_format_repairs_dedented_handle_in_clause() {
        let src = concat!(
            "bridge () =\n",
            "    result =\n",
            "        handle Log\n",
            "            line message =>\n",
            "                transcript := message\n",
            "    in\n",
            "        answer =\n",
            "            handle Prompt\n",
            "                ask field =>\n",
            "                    answer := field\n",
            "            in form ()\n",
            "        answer\n",
            "legacyTranscript = transcript\n",
        );
        let want = concat!(
            "bridge () =\n",
            "    result =\n",
            "        handle Log\n",
            "            line message =>\n",
            "                transcript := message\n",
            "        in\n",
            "            answer =\n",
            "                handle Prompt\n",
            "                    ask field =>\n",
            "                        answer := field\n",
            "                in form ()\n",
            "            answer\n",
            "    legacyTranscript = transcript\n",
        );

        assert_eq!(format_source(src, Flavor::Ml), Ok(want.to_owned()));
    }

    #[test]
    fn unparseable_source_is_reported_not_mangled() {
        let result = format_source("fn main( = {\n", Flavor::Default);
        assert!(result.is_err(), "{result:?}");
    }

    /// A candidate that reparses cleanly but to a *different* program is the
    /// dangerous shape: nothing downstream would notice. Reached directly,
    /// because the real formatter is not supposed to be able to produce it.
    #[test]
    fn a_candidate_that_changes_meaning_is_reported_not_returned_as_the_input() {
        let original = osprey_syntax::parse_program("let a = 1\n").program;
        let result = accept(&original, "let a = 2\n".to_owned(), Flavor::Default);
        assert_eq!(result, Err(vec![DECLINED.to_owned()]));
    }

    /// A candidate that no longer parses carries its own errors after the
    /// verdict, so the formatter defect can be located.
    #[test]
    fn an_unparseable_candidate_reports_the_verdict_and_its_errors() {
        let original = osprey_syntax::parse_program("let a = 1\n").program;
        let result = accept(&original, "let a = {\n".to_owned(), Flavor::Default);
        match result {
            Err(errors) => {
                assert_eq!(errors.first().map(String::as_str), Some(DECLINED));
                assert!(errors.len() > 1, "no candidate errors: {errors:?}");
            }
            Ok(text) => assert_eq!(text, DECLINED, "declined candidate accepted"),
        }
    }

    /// The accepting direction, so the two tests above cannot pass by the guard
    /// having been wired to reject everything.
    #[test]
    fn an_equivalent_candidate_is_returned_verbatim() {
        let original = osprey_syntax::parse_program("let a = 1\n").program;
        let candidate = "let  a  =  1\n".to_owned();
        assert_eq!(
            accept(&original, candidate.clone(), Flavor::Default),
            Ok(candidate)
        );
    }

    #[test]
    fn path_resolves_flavor_from_extension() {
        assert!(format_for_path("a.ospml", "main () =\n    print 1\n").is_ok());
        assert!(format_for_path("a.osp", "fn main() = {\n    print(1)\n}\n").is_ok());
    }

    #[test]
    fn finalize_collapses_blanks_and_trims_edges() {
        let lines = vec![
            String::new(),
            "a".to_owned(),
            String::new(),
            String::new(),
            "b".to_owned(),
            String::new(),
        ];
        assert_eq!(finalize(&lines), "a\n\nb\n");
    }

    #[test]
    fn finalize_of_nothing_is_empty() {
        assert_eq!(finalize(&[]), "");
        assert_eq!(finalize(&[String::new(), String::new()]), "");
    }

    #[test]
    fn indent_steps_in_four_space_units() {
        assert_eq!(indent_to(0), "");
        assert_eq!(indent_to(2), "        ");
        assert_eq!(indent_to(-3), "");
    }

    #[test]
    fn default_modules_preserve_file_namespace_paths_and_signature_blocks() {
        // [MODULES-FILE-SCOPED-NAMESPACE] Formatting changes indentation only;
        // the semicolon namespace and `::` qualification survive verbatim.
        let src = concat!(
            "namespace \"com.example/reports\";\n",
            "signature TaxApi {\n",
            "    fn rate() -> int\n",
            "}\n",
            "module Tax : TaxApi {\n",
            "    export fn rate() -> int = 10\n",
            "}\n",
            "import \"com.example/reports\" as reports\n",
            "let gross = reports::Tax::rate()\n",
        );
        let once = format_source(src, Flavor::Default).expect("formats modules");
        assert_eq!(once, src, "canonical module source is preserved");
        assert!(
            once.starts_with("namespace \"com.example/reports\";\n"),
            "{once}"
        );
        assert!(once.contains("    fn rate() -> int\n"), "{once}");
        assert!(once.contains("reports::Tax::rate()"), "{once}");
        assert_eq!(
            format_source(&once, Flavor::Default).expect("idempotent"),
            once
        );
    }

    #[test]
    fn ml_layout_import_members_and_modules_are_preserved_idempotently() {
        // [MODULES-IMPORT] The creamy ML surface keeps layout member imports;
        // `::` remains qualification and no Default braces/semicolons appear.
        let src = concat!(
            "namespace billing\n",
            "import billing::Tax\n",
            "    addTax\n",
            "    zero as noTax\n",
            "module Invoice\n",
            "    export total = addTax 100\n",
        );
        let once = format_source(src, Flavor::Ml).expect("formats ML modules");
        assert_eq!(once, src, "canonical ML module source is preserved");
        assert!(once.contains("import billing::Tax\n"), "{once}");
        assert!(once.contains("    addTax\n    zero as noTax\n"), "{once}");
        assert!(once.contains("module Invoice\n    export total"), "{once}");
        assert!(!once.contains('{') && !once.contains(';'), "{once}");
        assert_eq!(format_source(&once, Flavor::Ml).expect("idempotent"), once);
    }
}
