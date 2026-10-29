//! `[TYPE-UNION-POSITIONAL]` — positional constructions.
//!
//! A variant declared `Node(Tree, Tree)` has no field names to supply, so its
//! saturated application is a construction, not a call. [`construction`] is the
//! one builder of that node: both frontends fold through it while parsing, and
//! [`fold_constructions`] applies it again to a program assembled from units
//! parsed apart, where a constructor declared in one unit is applied in
//! another and the parser of the applying unit never saw its declaration.

use crate::mutate::{children_mut, statement_children_mut};
use crate::{is_positional_field, positional_field_name, Expr, FieldAssignment, Program, Stmt};
use std::collections::BTreeMap;

/// The construction node for `name` applied to its positional payload.
#[must_use]
pub fn construction(name: &str, arguments: Vec<Expr>) -> Expr {
    Expr::TypeConstructor {
        name: name.to_owned(),
        type_args: Vec::new(),
        fields: arguments
            .into_iter()
            .enumerate()
            .map(|(slot, value)| FieldAssignment {
                name: positional_field_name(slot),
                value,
            })
            .collect(),
    }
}

/// Every positionally-declared constructor among the top-level type
/// declarations in `statements`, with its arity.
#[must_use]
pub fn declared_arities(statements: &[Stmt]) -> BTreeMap<String, usize> {
    statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Type { variants, .. } => Some(variants),
            _ => None,
        })
        .flatten()
        .filter(|variant| {
            variant
                .fields
                .first()
                .is_some_and(|field| is_positional_field(&field.name))
        })
        .map(|variant| (variant.name.clone(), variant.fields.len()))
        .collect()
}

/// Rewrite every saturated call of a constructor in `arities` into its
/// construction. An unsaturated or over-applied call stays as written, so the
/// checker reports the arity mistake against what the author wrote.
pub fn fold_constructions(program: &mut Program, arities: &BTreeMap<String, usize>) {
    if arities.is_empty() {
        return;
    }
    let mut fold = |expression: &mut Expr| fold_expression(expression, arities);
    for statement in &mut program.statements {
        statement_children_mut(statement, &mut fold);
    }
}

fn fold_expression(expression: &mut Expr, arities: &BTreeMap<String, usize>) {
    children_mut(expression, &mut |child| fold_expression(child, arities));
    let Some((name, count)) = spine_head(expression) else {
        return;
    };
    if arities.get(&name) != Some(&count) {
        return;
    }
    let spine = std::mem::replace(expression, Expr::Identifier(String::new()));
    *expression = construction(&name, unwind(spine));
}

/// The constructor name and total argument count of a call spine: one flat
/// call `Both(a, b)`, or the curried spine `((Both a) b)` the ML frontend
/// keeps for a qualified constructor whose arity it could not see.
fn spine_head(expression: &Expr) -> Option<(String, usize)> {
    let Expr::Call {
        function,
        arguments,
        named_arguments,
    } = expression
    else {
        return None;
    };
    if !named_arguments.is_empty() {
        return None;
    }
    match function.as_ref() {
        Expr::Identifier(name) => Some((name.clone(), arguments.len())),
        inner if arguments.len() == 1 => spine_head(inner).map(|(name, count)| (name, count + 1)),
        _ => None,
    }
}

/// The arguments of a call spine in source order.
fn unwind(expression: Expr) -> Vec<Expr> {
    match expression {
        Expr::Call {
            function,
            arguments,
            ..
        } => {
            let mut all = unwind(*function);
            all.extend(arguments);
            all
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, arguments: Vec<Expr>) -> Expr {
        Expr::Call {
            function: Box::new(Expr::Identifier(name.to_owned())),
            arguments,
            named_arguments: Vec::new(),
        }
    }

    fn slots(expression: &Expr) -> Option<(&str, Vec<&str>)> {
        match expression {
            Expr::TypeConstructor { name, fields, .. } => Some((
                name.as_str(),
                fields.iter().map(|field| field.name.as_str()).collect(),
            )),
            _ => None,
        }
    }

    fn folded(statement: &Stmt) -> Option<&Expr> {
        match statement {
            Stmt::Expr { value, .. } => Some(value),
            _ => None,
        }
    }

    #[test]
    fn a_curried_spine_of_a_qualified_constructor_folds_when_saturated() {
        let arities = BTreeMap::from([("Pair".to_owned(), 2)]);
        let spine = Expr::Call {
            function: Box::new(call("Pair", vec![Expr::Identifier("a".to_owned())])),
            arguments: vec![Expr::Identifier("b".to_owned())],
            named_arguments: Vec::new(),
        };
        let mut program = Program {
            statements: vec![Stmt::Expr {
                value: spine,
                doc: None,
                position: None,
            }],
            doc: None,
        };
        fold_constructions(&mut program, &arities);
        let value = program.statements.first().and_then(folded);
        assert_eq!(value.and_then(slots), Some(("Pair", vec!["0", "1"])));
        let over = Expr::Call {
            function: Box::new(call("Pair", vec![Expr::Identifier("a".to_owned())])),
            arguments: vec![
                Expr::Identifier("b".to_owned()),
                Expr::Identifier("c".to_owned()),
            ],
            named_arguments: Vec::new(),
        };
        let mut program = Program {
            statements: vec![Stmt::Expr {
                value: over,
                doc: None,
                position: None,
            }],
            doc: None,
        };
        fold_constructions(&mut program, &arities);
        assert!(program
            .statements
            .first()
            .and_then(folded)
            .and_then(slots)
            .is_none());
    }

    #[test]
    fn saturated_calls_fold_nested_and_others_stay_calls() {
        let arities = BTreeMap::from([("Pair".to_owned(), 2), ("Box".to_owned(), 1)]);
        let inner = call("Box", vec![Expr::Identifier("x".to_owned())]);
        let statement = |value| Stmt::Expr {
            value,
            doc: None,
            position: None,
        };
        let mut program = Program {
            statements: vec![
                statement(call("Pair", vec![inner.clone(), call("Box", vec![])])),
                statement(call("Other", vec![inner])),
            ],
            doc: None,
        };
        fold_constructions(&mut program, &arities);
        let mut statements = program.statements.iter().filter_map(folded);
        let pair = statements.next();
        assert_eq!(pair.and_then(slots), Some(("Pair", vec!["0", "1"])));
        let payload: Vec<&Expr> = match pair {
            Some(Expr::TypeConstructor { fields, .. }) => {
                fields.iter().map(|field| &field.value).collect()
            }
            _ => Vec::new(),
        };
        assert_eq!(
            payload.first().and_then(|value| slots(value)),
            Some(("Box", vec!["0"])),
            "nested saturated call folds"
        );
        assert!(
            matches!(payload.get(1), Some(Expr::Call { .. })),
            "an unsaturated call stays a call"
        );
        let argument = match statements.next() {
            Some(Expr::Call { arguments, .. }) => arguments.first(),
            _ => None,
        };
        assert_eq!(
            argument.and_then(slots),
            Some(("Box", vec!["0"])),
            "arguments of any call fold"
        );
    }

    fn variant(name: &str, fields: &[&str]) -> crate::TypeVariant {
        crate::TypeVariant {
            name: name.to_owned(),
            fields: fields
                .iter()
                .map(|field| crate::TypeField {
                    name: (*field).to_owned(),
                    ty: "int".to_owned(),
                    constraint: None,
                    effects: None,
                })
                .collect(),
        }
    }

    /// Only payloads declared by position fold; a named-field variant keeps
    /// its record construction and a fieldless one has nothing to fold.
    #[test]
    fn arities_come_from_positional_variants_only() {
        let declared = Stmt::Type {
            name: "Shape".to_owned(),
            type_params: Vec::new(),
            variants: vec![
                variant("Pair", &["0", "1"]),
                variant("Named", &["x"]),
                variant("Empty", &[]),
            ],
            alias: None,
            validation_func: None,
            opaque: false,
            doc: None,
            position: None,
        };
        let other = Stmt::Expr {
            value: Expr::Integer(0),
            doc: None,
            position: None,
        };
        let arities = declared_arities(&[declared, other]);
        assert_eq!(arities, BTreeMap::from([("Pair".to_owned(), 2)]));
    }
}
