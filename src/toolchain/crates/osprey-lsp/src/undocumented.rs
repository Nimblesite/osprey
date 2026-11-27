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
    path: &std::path::Path,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    osprey_types::source_warnings(program, source, flavor, path)
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

    /// A temporary project, made a package when `package` holds, with
    /// `[lints]` lines appended to its manifest.
    fn project(package: bool, lints: &str) -> crate::test_support::ProjectFixture {
        let fixture = crate::test_support::ProjectFixture::new("ospml").expect("fixture");
        let manifest = fixture.root.join("osprey.toml");
        let mut text = std::fs::read_to_string(&manifest).expect("manifest");
        if package {
            text.push_str("[package]\nname = \"@o/api\"\n");
        }
        text = format!("{text}[lints]\n{lints}");
        std::fs::write(&manifest, text).expect("manifest");
        let _entry = fixture.write("main", "print \"ready\"\n").expect("entry");
        fixture
    }

    /// `(code, line, severity)` of every finding in `text`, written at
    /// `relative` under the fixture.
    fn findings(
        fixture: &crate::test_support::ProjectFixture,
        relative: &str,
        text: &str,
    ) -> Vec<(String, u32, Severity)> {
        let path = fixture.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
        std::fs::write(&path, text).expect("source");
        let uri = lspkit_server::uri::path_to_uri(&path).expect("uri");
        crate::diagnostics::compute(text, &uri, PositionEncoding::Utf16)
            .into_iter()
            .map(|d| (d.code.unwrap_or_default(), d.range.0, d.severity))
            .collect()
    }

    const DOCUMENTED: &str =
        "(** Doubling. *)\nmodule Api\n    (** Twice `x`. *)\n    export double x = x * 2\n";
    const BARE: &str = "(** Doubling. *)\nmodule Api\n    export double x = x * 2\n";

    fn at(code: &str, line: u32, severity: Severity) -> (String, u32, Severity) {
        (code.to_owned(), line, severity)
    }

    use lspkit_server::Severity;

    #[test]
    fn a_package_export_without_docs_or_without_an_example_is_an_editor_error() {
        // Implements [LINT-LEVELS], [DOC-EXPORT-REQUIRED], [DOC-EXPORT-EXAMPLE-REQUIRED].
        let package = project(true, "");
        let bare = [at("undocumented-export", 2, Severity::Error)];
        let unexampled = [at("unexampled-export", 3, Severity::Error)];
        assert_eq!(findings(&package, "src/api.ospml", BARE), bare);
        assert_eq!(findings(&package, "src/api.ospml", DOCUMENTED), unexampled);
        assert_eq!(
            findings(&package, "lib/api.ospml", BARE),
            bare,
            "outside the roots"
        );
    }

    #[test]
    fn an_application_and_a_manifest_free_file_get_no_documentation_advice() {
        // Implements [LINT-LEVELS]: an application's exports are its own.
        let app = project(false, "");
        assert_eq!(findings(&app, "src/api.ospml", BARE), []);
        assert_eq!(findings(&app, "src/api.ospml", DOCUMENTED), []);
        assert_eq!(codes(BARE), []);
        assert_eq!(codes(DOCUMENTED), []);
    }

    #[test]
    fn the_lints_table_sets_any_rules_editor_severity() {
        // Implements [LINT-LEVELS].
        let strict_app = project(
            false,
            "undocumented-export = \"deny\"\nunexampled-export = \"warn\"\n",
        );
        assert_eq!(
            findings(&strict_app, "src/api.ospml", BARE),
            [at("undocumented-export", 2, Severity::Error)]
        );
        assert_eq!(
            findings(&strict_app, "src/api.ospml", DOCUMENTED),
            [at("unexampled-export", 3, Severity::Warning)]
        );
        let lax_package = project(
            true,
            "undocumented-export = \"info\"\nunexampled-export = \"allow\"\n",
        );
        assert_eq!(
            findings(&lax_package, "src/api.ospml", BARE),
            [at("undocumented-export", 2, Severity::Information)]
        );
        assert_eq!(findings(&lax_package, "src/api.ospml", DOCUMENTED), []);
        let pyramid = chains().0;
        let quiet = project(false, "closing-run = \"allow\"\n");
        assert_eq!(findings(&quiet, "src/main.ospml", &pyramid), []);
        let loud = project(false, "closing-run = \"deny\"\n");
        assert_eq!(
            findings(&loud, "src/main.ospml", &pyramid),
            [at("closing-run", 1, Severity::Error)]
        );
    }

    #[test]
    fn test_support_needs_documentation_but_no_examples() {
        // [DOC-EXPORT-EXAMPLE-TEST-SUPPORT]: a module under a `tests` or
        // `test` folder serves its suite, not readers of the API.
        let package = project(true, "");
        for relative in [
            "tests/api.ospml",
            "test/api.ospml",
            "lib/tests/deep/api.ospml",
        ] {
            assert_eq!(findings(&package, relative, DOCUMENTED), [], "{relative}");
            assert_eq!(
                findings(&package, relative, BARE),
                [at("undocumented-export", 2, Severity::Error)],
                "{relative}"
            );
        }
        for relative in ["src/api.ospml", "contests/api.ospml", "tests.ospml"] {
            assert_eq!(
                findings(&package, relative, DOCUMENTED),
                [at("unexampled-export", 3, Severity::Error)],
                "{relative}"
            );
        }
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
        let codes: Vec<String> = findings(&project(true, ""), "src/api.ospml", source)
            .into_iter()
            .map(|(code, ..)| code)
            .collect();
        assert_eq!(codes, ["type-error"]);
    }
}
