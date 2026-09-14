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
}
