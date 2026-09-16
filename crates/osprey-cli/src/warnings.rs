//! Terminal rendering for compiler warnings.
//!
//! A warning is advice, and a hundred of them are only useful if a reader can
//! see the shape of the advice at a glance. Warnings are therefore grouped by
//! the file they were written in, listed under an aligned `line:column`
//! gutter, and closed with the count and the rules that raised them, so the
//! summary answers "how much of this is there, and what turned it on" without
//! scrolling back.
//!
//! Rendering is a pure function over the diagnostics so the exact text is
//! testable; printing is the only side effect.

use std::collections::{BTreeMap, BTreeSet};

use osprey_types::TypeWarning;

use crate::project::CompilationInput;

/// One file's warnings: the `line:column` gutter and the message, in order.
type Listing = Vec<(String, String)>;

/// Compiler warnings share one terminal listing and never affect exit status.
pub(crate) fn collect(program: &osprey_ast::Program) -> Vec<TypeWarning> {
    let mut warnings = osprey_types::redundant_annotations(program);
    warnings.extend(
        osprey_types::unused_symbols(program)
            .into_iter()
            .map(|symbol| symbol.warning),
    );
    warnings.sort_by_key(|warning| {
        warning
            .position
            .map(|position| (position.line, position.column))
    });
    warnings
}

/// Print `warnings` to stderr. A clean build prints nothing.
pub(crate) fn report(input: &CompilationInput, warnings: &[TypeWarning]) {
    if let Some(text) = render(input, warnings) {
        eprintln!("{text}");
    }
}

/// The whole warning block, or `None` when there is nothing to say.
pub(crate) fn render(input: &CompilationInput, warnings: &[TypeWarning]) -> Option<String> {
    if warnings.is_empty() {
        return None;
    }
    let blocks: Vec<String> = group(input, warnings)
        .into_iter()
        .map(|(file, listing)| block(&file, &listing))
        .collect();
    Some(format!("{}\n{}", blocks.join("\n"), summary(warnings)))
}

/// One file's heading and its aligned listing.
fn block(file: &str, listing: &Listing) -> String {
    let width = listing
        .iter()
        .map(|(gutter, _)| gutter.len())
        .max()
        .unwrap_or_default();
    let lines = listing
        .iter()
        .map(|(gutter, message)| format!("  {gutter:>width$}  warning: {message}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("\n{file}\n{lines}\n")
}

/// Group warnings by file, keeping each file's own source order.
fn group(input: &CompilationInput, warnings: &[TypeWarning]) -> BTreeMap<String, Listing> {
    let mut grouped: BTreeMap<String, Listing> = BTreeMap::new();
    for warning in warnings {
        let (file, gutter) = locate(input, warning);
        grouped
            .entry(file)
            .or_default()
            .push((gutter, warning.message.clone()));
    }
    grouped
}

/// A warning's file and `line:column` gutter, falling back to the unit's own
/// label for a warning inference could not anchor to a position.
fn locate(input: &CompilationInput, warning: &TypeWarning) -> (String, String) {
    match warning.position {
        Some(position) => {
            let (file, line, column) = input.location(position);
            (file, format!("{line}:{column}"))
        }
        None => (input.display_path().to_string(), String::from("-")),
    }
}

/// The closing line: how many warnings there are and which rules raised them.
fn summary(warnings: &[TypeWarning]) -> String {
    let rules: BTreeSet<&str> = warnings.iter().map(|w| w.rule).collect();
    let plural = if warnings.len() == 1 { "" } else { "s" };
    format!(
        "{} warning{plural} ({})",
        warnings.len(),
        rules.into_iter().collect::<Vec<_>>().join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::project::CompilationInput;
    use osprey_ast::Position;
    use osprey_types::{TypeWarning, REDUNDANT_ANNOTATION};

    /// A single-file unit whose warnings resolve against `main.osp`.
    fn unit() -> CompilationInput {
        let program = osprey_syntax::parse_program("let answer = 42\n").program;
        CompilationInput::script("main.osp", String::new(), program)
    }

    fn warning(line: u32, column: u32, message: &str) -> TypeWarning {
        TypeWarning {
            message: message.to_string(),
            position: Some(Position { line, column }),
            rule: REDUNDANT_ANNOTATION,
        }
    }

    #[test]
    fn a_clean_program_renders_nothing_at_all() {
        assert_eq!(render(&unit(), &[]), None);
    }

    #[test]
    fn one_warning_renders_its_file_gutter_message_and_singular_summary() {
        let raised = [warning(3, 4, "redundant return type annotation on `greet`")];
        assert_eq!(
            render(&unit(), &raised),
            Some(
                "\nmain.osp\n  3:4  warning: redundant return type annotation on `greet`\n\n1 warning (redundant-annotation)"
                    .to_string()
            )
        );
    }

    #[test]
    fn gutters_are_right_aligned_so_messages_line_up() {
        let raised = [warning(9, 1, "first"), warning(100, 12, "second")];
        assert_eq!(
            render(&unit(), &raised),
            Some(
                "\nmain.osp\n     9:1  warning: first\n  100:12  warning: second\n\n2 warnings (redundant-annotation)"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_positionless_warning_still_lists_under_its_unit() {
        let raised = [TypeWarning {
            message: String::from("nowhere in particular"),
            position: None,
            rule: REDUNDANT_ANNOTATION,
        }];
        assert_eq!(
            render(&unit(), &raised),
            Some(
                "\nmain.osp\n  -  warning: nowhere in particular\n\n1 warning (redundant-annotation)"
                    .to_string()
            )
        );
    }

    #[test]
    fn the_summary_lists_every_rule_that_fired_once_each() {
        let raised = [
            warning(1, 0, "a"),
            warning(2, 0, "b"),
            TypeWarning {
                message: String::from("c"),
                position: Some(Position { line: 3, column: 0 }),
                rule: "some-other-rule",
            },
        ];
        let text = render(&unit(), &raised).unwrap_or_default();
        assert!(
            text.ends_with("\n3 warnings (redundant-annotation, some-other-rule)"),
            "{text}"
        );
    }

    #[test]
    fn compiler_collects_redundancy_and_unused_advice_without_losing_either_rule() {
        let source = "fn choose(value, ignored) = {\n let spare: int = 1\n value\n}\nlet result = choose(2, 3)\n";
        let parsed = osprey_syntax::parse_program(source);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert!(osprey_types::check_program(&parsed.program).is_empty());
        let warnings = super::collect(&parsed.program);
        let input = CompilationInput::script("mixed.osp", source.to_owned(), parsed.program);
        assert_eq!(render(&input, &warnings), Some(
            "\nmixed.osp\n  1:3  warning: unused parameter `ignored`\n  2:1  warning: redundant type annotation on `spare`: inference derives `int` without it\n  2:1  warning: unused variable `spare`\n\n3 warnings (redundant-annotation, unused-parameter, unused-variable)".to_owned()
        ));
    }

    #[test]
    fn signed_inline_unused_parameters_never_report_compiler_generated_locals() {
        let parsed = osprey_syntax::parse_program_with_flavor(
            "choose : int -> int -> int\nchoose (unused : int) (used : int) = used\nresult = choose 1 2\n",
            osprey_syntax::Flavor::Ml,
        );
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let warnings = super::collect(&parsed.program);
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.rule.starts_with("unused-"))
                .map(|warning| (warning.rule, warning.message.as_str()))
                .collect::<Vec<_>>(),
            [("unused-parameter", "unused parameter `unused`")]
        );
        assert_eq!(
            crate::report_type_errors(&CompilationInput::script(
                "signed.ospml",
                String::new(),
                parsed.program
            )),
            0
        );
    }

    #[test]
    fn an_invalid_program_retains_errors_and_has_no_speculative_advice() {
        let parsed = osprey_syntax::parse_program("fn broken(unused) = missing\n");
        assert!(parsed.errors.is_empty());
        assert!(super::collect(&parsed.program).is_empty());
        assert_eq!(
            crate::report_type_errors(&CompilationInput::script(
                "broken.osp",
                String::new(),
                parsed.program
            )),
            1
        );
    }

    #[test]
    fn assembled_handler_messages_use_the_source_effect_name() {
        let source = "namespace test;\neffect Vault { balance: fn(int) -> int }\nfn total() = handle Vault\n balance id => 0\nin perform Vault.balance(1)\nlet result = total()\n";
        let parsed = osprey_syntax::parse_program(source);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let input = CompilationInput::one_source(
            "warning.osp",
            osprey_syntax::Flavor::Default,
            source.to_owned(),
            parsed.program,
        )
        .unwrap_or_else(|errors| panic!("{errors:?}"));
        let warnings = super::collect(input.program());
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.rule.starts_with("unused-"))
                .map(|warning| (warning.rule, warning.message.as_str()))
                .collect::<Vec<_>>(),
            [(
                "unused-handler-parameter",
                "unused handler parameter `id` of `test::Vault.balance`"
            )]
        );
        assert_eq!(crate::report_type_errors(&input), 0);
    }

    #[test]
    fn top_level_pattern_warnings_have_source_locations_in_both_flavors() {
        for (flavor, source) in [
            (
                osprey_syntax::Flavor::Default,
                "match 1 { unused => print(\"ok\") }\n",
            ),
            (
                osprey_syntax::Flavor::Ml,
                "match 1\n    unused => print \"ok\"\n",
            ),
        ] {
            let parsed = osprey_syntax::parse_program_with_flavor(source, flavor);
            assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
            assert!(osprey_types::check_program(&parsed.program).is_empty());
            let warnings = super::collect(&parsed.program);
            let input = CompilationInput::script("top-level", source.to_owned(), parsed.program);
            assert_eq!(render(&input, &warnings), Some("\ntop-level\n  1:0  warning: unused pattern binding `unused`\n\n1 warning (unused-pattern-binding)".to_owned()));
        }
    }
}
