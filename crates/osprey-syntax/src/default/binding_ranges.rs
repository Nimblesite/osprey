//! Name tokens for lexical declarations, taken only from binding CST nodes.

use osprey_ast::Position;
use tree_sitter::Node;

use crate::{BindingKind, BindingRange};

pub(crate) fn collect(source: &str) -> Vec<BindingRange> {
    scan(source).0
}

pub(crate) fn literals(source: &str) -> Vec<crate::fragment_ranges::Literal> {
    scan(source).1
}

fn scan(source: &str) -> (Vec<BindingRange>, Vec<crate::fragment_ranges::Literal>) {
    let Some(tree) = super::parse_tree(source) else {
        return (Vec::new(), Vec::new());
    };
    let mut ranges = Vec::new();
    let mut literals = Vec::new();
    walk(tree.root_node(), source, None, &mut ranges, &mut literals);
    (ranges, literals)
}

fn position(node: Node<'_>) -> Position {
    super::position_from_point(node.start_position())
}

fn walk(
    node: Node<'_>,
    source: &str,
    owner: Option<Position>,
    out: &mut Vec<BindingRange>,
    literals: &mut Vec<crate::fragment_ranges::Literal>,
) {
    let owner = match node.kind() {
        "function_declaration" => node.child_by_field_name("name").map(position),
        "let_declaration" => node.child_by_field_name("keyword").map(position),
        "lambda_expression" => Some(position(node)),
        "handler_arm" => node.child_by_field_name("operation").map(position),
        "expression_statement" if owner.is_none() => expression_position(node),
        _ => owner,
    };
    match node.kind() {
        "extern_declaration" | "signature_declaration" => return,
        "string" | "interpolated_string" => literals.push(crate::fragment_ranges::Literal {
            range: node.byte_range(),
            position: position(node),
            owner_position: owner,
        }),
        "let_declaration" => add_name(node, source, owner, BindingKind::Variable, out),
        "parameter" => add_name(node, source, owner, BindingKind::Parameter, out),
        "handler_params" => {
            add_identifiers(node, source, owner, BindingKind::HandlerParameter, out);
        }
        "match_arm" | "select_arm" => {
            if let Some(pattern) = node.child_by_field_name("pattern") {
                pattern_ranges(pattern, source, owner, out);
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "pattern")
    {
        walk(child, source, owner, out, literals);
    }
}

fn expression_position(node: Node<'_>) -> Option<Position> {
    let mut cursor = node.walk();
    let expression = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "expression");
    expression.map(position)
}

fn add_name(
    node: Node<'_>,
    source: &str,
    owner: Option<Position>,
    kind: BindingKind,
    out: &mut Vec<BindingRange>,
) {
    if let Some(name) = node.child_by_field_name("name") {
        add(name, source, owner, kind, out);
    }
}

fn add_identifiers(
    node: Node<'_>,
    source: &str,
    owner: Option<Position>,
    kind: BindingKind,
    out: &mut Vec<BindingRange>,
) {
    let mut cursor = node.walk();
    for name in node
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "identifier")
    {
        add(name, source, owner, kind, out);
    }
}

fn add(
    node: Node<'_>,
    source: &str,
    owner: Option<Position>,
    kind: BindingKind,
    out: &mut Vec<BindingRange>,
) {
    let Ok(name) = node.utf8_text(source.as_bytes()) else {
        return;
    };
    let occurrence = out
        .iter()
        .filter(|binding| {
            binding.owner_position == owner && binding.kind == kind && binding.name == name
        })
        .count();
    out.push(BindingRange {
        owner_position: owner,
        kind,
        name: name.to_owned(),
        occurrence,
        range: node.byte_range(),
    });
}

fn pattern_ranges(
    node: Node<'_>,
    source: &str,
    owner: Option<Position>,
    out: &mut Vec<BindingRange>,
) {
    match node.kind() {
        "field_pattern" | "tuple_pattern" => {
            add_identifiers(node, source, owner, BindingKind::PatternBinding, out);
        }
        "pattern" if bare_pattern(node) => {
            add_name(node, source, owner, BindingKind::PatternBinding, out);
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor).filter(|child| {
        matches!(
            child.kind(),
            "pattern" | "list_pattern" | "structural_pattern" | "field_pattern" | "tuple_pattern"
        )
    }) {
        pattern_ranges(child, source, owner, out);
    }
    if node.kind() == "list_pattern" {
        if let Some(rest) = node.child_by_field_name("rest") {
            add(rest, source, owner, BindingKind::PatternBinding, out);
        }
    }
}

fn bare_pattern(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let has_children = node
        .named_children(&mut cursor)
        .any(|child| matches!(child.kind(), "pattern" | "field_pattern"));
    !has_children
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(source: &str) -> Vec<BindingRange> {
        let parsed = crate::parse_program(source);
        assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
        collect(source)
    }

    #[test]
    fn declaration_tokens_exclude_comments_types_and_body_reads() {
        let source = "fn choose(value: int, ignored) = {\n // ignored value\n let local = value\n \"ignored ${local}\"\n}\n";
        let found = ranges(source);
        assert_eq!(
            found
                .iter()
                .map(|b| (b.kind, b.name.as_str(), &source[b.range.clone()]))
                .collect::<Vec<_>>(),
            [
                (BindingKind::Parameter, "value", "value"),
                (BindingKind::Parameter, "ignored", "ignored"),
                (BindingKind::Variable, "local", "local")
            ]
        );
        assert_eq!(
            found.iter().map(|b| b.owner_position).collect::<Vec<_>>(),
            [
                Some(Position { line: 1, column: 3 }),
                Some(Position { line: 1, column: 3 }),
                Some(Position { line: 3, column: 1 })
            ]
        );
    }

    #[test]
    fn repeated_arm_names_get_distinct_exact_declaration_ranges() {
        let source =
            "fn choose(value) = match value {\n Left(item) => item\n Right(item) => 0\n}\n";
        let found: Vec<_> = ranges(source)
            .into_iter()
            .filter(|b| b.kind == BindingKind::PatternBinding)
            .collect();
        assert_eq!(
            found
                .iter()
                .map(|b| (&source[b.range.clone()], b.occurrence))
                .collect::<Vec<_>>(),
            [("item", 0), ("item", 1)]
        );
        assert_eq!(
            found.iter().map(|b| b.range.start).collect::<Vec<_>>(),
            [
                source.find("item)").unwrap_or_default(),
                source.rfind("item)").unwrap_or_default()
            ]
        );
    }

    #[test]
    fn patterns_handlers_lambdas_and_nested_owners_are_separate() {
        let source = "effect Pair { choose: fn(int, int) -> int }\nfn run(value) = handle Pair\n choose used unused => match value { [head, ...tail] => used\n [] => 0 }\nin { let callback = fn(value) => value\n callback(1) }\n";
        let found = ranges(source);
        for (name, kind, owner) in [
            (
                "unused",
                BindingKind::HandlerParameter,
                Position { line: 3, column: 1 },
            ),
            (
                "tail",
                BindingKind::PatternBinding,
                Position { line: 3, column: 1 },
            ),
            (
                "callback",
                BindingKind::Variable,
                Position { line: 5, column: 5 },
            ),
            (
                "value",
                BindingKind::Parameter,
                Position {
                    line: 5,
                    column: 20,
                },
            ),
        ] {
            assert!(
                found.iter().any(|b| b.name == name
                    && b.kind == kind
                    && b.owner_position == Some(owner)
                    && &source[b.range.clone()] == name),
                "{name}: {found:?}"
            );
        }
    }
}
