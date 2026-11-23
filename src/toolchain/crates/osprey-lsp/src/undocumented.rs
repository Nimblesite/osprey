//! Per-file advice as editor warnings: undocumented and unexampled exports
//! and repeated literals. Implements [DOC-EXPORT-REQUIRED],
//! [DOC-EXPORT-EXAMPLE-REQUIRED] and [TYPE-WARNINGS-REPEATED-LITERAL].

use lspkit::vfs::PositionEncoding;
use lspkit_server::Diagnostic;
use osprey_ast::Program;

/// One warning per undocumented or unexampled export and repeated literal in this file, read from the file as
/// written: project assembly flattens modules away.
pub(crate) fn diagnostics(
    source: &str,
    program: &Program,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    osprey_types::source_warnings(program)
        .iter()
        .filter_map(|export| {
            let position = export.position?;
            Some(crate::diagnostics::warning(
                source,
                position,
                &export.message,
                export.rule,
                encoding,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use lspkit::vfs::PositionEncoding;

    fn codes(source: &str) -> Vec<(String, u32)> {
        crate::diagnostics::compute(source, "api.ospml", PositionEncoding::Utf16)
            .into_iter()
            .map(|d| (d.code.unwrap_or_default(), d.range.0))
            .collect()
    }

    #[test]
    fn an_undocumented_export_is_an_editor_warning_on_its_line() {
        let source = "(** Doubling. *)\nmodule Api\n    export double x = x * 2\n";
        assert_eq!(codes(source), [("undocumented-export".to_owned(), 2)]);
    }

    #[test]
    fn a_documented_export_without_an_example_is_an_editor_warning_on_its_line() {
        let source =
            "(** Doubling. *)\nmodule Api\n    (** Twice `x`. *)\n    export double x = x * 2\n";
        assert_eq!(codes(source), [("unexampled-export".to_owned(), 3)]);
    }

    #[test]
    fn a_literal_written_three_times_is_an_editor_warning_at_its_first_use() {
        let source = "a () = \"connection refused\"\nb () = \"connection refused\"\nc () = \"connection refused\"\n";
        assert_eq!(codes(source), [("repeated-literal".to_owned(), 0)]);
    }

    #[test]
    fn documentation_advice_waits_for_the_file_to_check() {
        let source = "module Api\n    export double x = missing x\n";
        let codes: Vec<String> = codes(source).into_iter().map(|(code, _)| code).collect();
        assert_eq!(codes, ["type-error"]);
    }
}
