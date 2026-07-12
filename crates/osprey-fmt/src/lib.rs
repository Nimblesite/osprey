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
//!   candidate fails to parse), the original source is returned untouched. The
//!   formatter therefore can never change what a program does — at worst it makes
//!   no change.
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
/// input does not parse; an unparseable file is never reformatted.
pub fn format_source(src: &str, flavor: Flavor) -> Result<String, Vec<String>> {
    let parsed = osprey_syntax::parse_program_with_flavor(src, flavor);
    if !parsed.errors.is_empty() {
        return Err(parsed.errors.iter().map(error_line).collect());
    }
    let candidate = match flavor {
        Flavor::Default => brace::format(src),
        Flavor::Ml => layout::format(src),
    };
    if preserves_meaning(&parsed.program, &candidate, flavor) {
        Ok(candidate)
    } else {
        Ok(src.to_string())
    }
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

/// Whether `candidate` reparses to exactly the same program as the original —
/// the guard that makes formatting meaning-preserving.
fn preserves_meaning(original: &osprey_ast::Program, candidate: &str, flavor: Flavor) -> bool {
    let reparsed = osprey_syntax::parse_program_with_flavor(candidate, flavor);
    reparsed.errors.is_empty() && &reparsed.program == original
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
    fn unparseable_source_is_reported_not_mangled() {
        let result = format_source("fn main( = {\n", Flavor::Default);
        assert!(result.is_err(), "{result:?}");
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
