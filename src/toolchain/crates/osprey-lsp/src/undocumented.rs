//! Undocumented exports as editor warnings. Implements [DOC-EXPORT-REQUIRED].

use lspkit::vfs::PositionEncoding;
use lspkit_server::Diagnostic;
use osprey_ast::Program;

/// One warning per undocumented export in this file, read from the file as
/// written: project assembly flattens modules away.
pub(crate) fn diagnostics(
    source: &str,
    program: &Program,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    osprey_types::undocumented_exports(program)
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
    fn documentation_advice_waits_for_the_file_to_check() {
        let source = "module Api\n    export double x = missing x\n";
        let codes: Vec<String> = codes(source).into_iter().map(|(code, _)| code).collect();
        assert_eq!(codes, ["type-error"]);
    }
}
