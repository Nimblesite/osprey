//! Per-file advice as editor warnings: undocumented and unexampled exports,
//! repeated literals, consecutive prints and callback pyramids. Implements
//! [DOC-EXPORT-REQUIRED], [DOC-EXPORT-EXAMPLE-REQUIRED],
//! [TYPE-WARNINGS-REPEATED-LITERAL], [TYPE-WARNINGS-CONSECUTIVE-PRINT] and
//! [LINT-NESTING-LAMBDA].

use lspkit::vfs::PositionEncoding;
use lspkit_server::Diagnostic;
use osprey_ast::Program;
use osprey_syntax::Flavor;

/// One warning per piece of per-file advice, read from the file as written:
/// project assembly flattens modules away.
pub(crate) fn diagnostics(
    source: &str,
    program: &Program,
    flavor: Flavor,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    osprey_types::source_warnings(program, source, flavor)
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
    fn a_documented_export_without_an_example_is_editor_information_on_its_line() {
        let source =
            "(** Doubling. *)\nmodule Api\n    (** Twice `x`. *)\n    export double x = x * 2\n";
        assert_eq!(codes(source), [("unexampled-export".to_owned(), 3)]);
        let found = crate::diagnostics::compute(source, "api.ospml", PositionEncoding::Utf16);
        assert_eq!(found[0].severity, lspkit_server::Severity::Information);
        let undocumented = "(** Doubling. *)\nmodule Api\n    export double x = x * 2\n";
        let found = crate::diagnostics::compute(undocumented, "api.ospml", PositionEncoding::Utf16);
        assert_eq!(found[0].severity, lspkit_server::Severity::Warning);
    }

    #[test]
    fn a_literal_written_three_times_is_an_editor_warning_at_its_first_use() {
        let source = "a () = \"connection refused\"\nb () = \"connection refused\"\nc () = \"connection refused\"\n";
        assert_eq!(codes(source), [("repeated-literal".to_owned(), 0)]);
    }

    #[test]
    fn prints_in_a_row_are_one_editor_warning_at_the_first() {
        let source = "x = 1\nprint \"a\"\nprint \"b\"\nprint \"${x}\"\n";
        assert_eq!(codes(source), [("consecutive-print".to_owned(), 1)]);
    }

    /// Every finding's message in `source`, in order.
    fn messages(source: &str) -> Vec<String> {
        crate::diagnostics::compute(source, "api.ospml", PositionEncoding::Utf16)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    /// The closing-run advice for a run of `n` brackets.
    fn run_message(n: usize) -> String {
        format!("{n} brackets close here at once: this code nests {n} deep; flatten its callbacks into `use x <- f(...)` lines and name its inner values")
    }

    /// Eight `andThen` binds as a pyramid and as a flat `use` block.
    fn chains() -> (String, String) {
        let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
        let step = "step (i, n) = Success(value = n + i)\n";
        let done = format!("Success(value = {})", names.join(" + "));
        let binds = names.iter().zip(1..);
        let nested = binds
            .clone()
            .map(|(v, i)| format!("andThen (step ({i}, n), \\{v} => "))
            .collect::<Vec<_>>()
            .concat();
        let lines = binds
            .map(|(v, i)| format!("    use {v} <- andThen (step ({i}, n))\n"))
            .collect::<Vec<_>>()
            .concat();
        let closers = ")".repeat(names.len());
        let pyramid = format!("{step}chain n = {nested}{done}{closers}\n");
        (pyramid, format!("{step}chain n =\n{lines}    {done}\n"))
    }

    #[test]
    fn a_callback_pyramid_is_an_editor_warning_and_its_flat_use_block_is_not() {
        let (pyramid, flat) = chains();
        assert_eq!(
            codes(&pyramid),
            [("closing-run".to_owned(), 1)],
            "{pyramid}"
        );
        assert_eq!(messages(&pyramid), [run_message(9)], "{pyramid}");
        assert!(codes(&flat).is_empty(), "{flat}");
    }

    #[test]
    fn deeply_nested_data_is_a_warning_too_and_one_level_less_is_not() {
        let data = |depth: usize| format!("xs = {}1{}\n", "[".repeat(depth), "]".repeat(depth));
        assert!(codes(&data(7)).is_empty(), "{}", data(7));
        assert_eq!(codes(&data(8)), [("closing-run".to_owned(), 0)]);
        assert_eq!(messages(&data(8)), [run_message(8)]);
        assert_eq!(messages(&data(12)), [run_message(12)]);
    }

    #[test]
    fn documentation_advice_waits_for_the_file_to_check() {
        let source = "module Api\n    export double x = missing x\n";
        let codes: Vec<String> = codes(source).into_iter().map(|(code, _)| code).collect();
        assert_eq!(codes, ["type-error"]);
    }
}
