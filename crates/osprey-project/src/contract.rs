//! Structural module-signature conformance checks.
//! Implements [MODULES-SIGNATURE] and [MODULES-EFFECTS].

use osprey_ast::{
    EffectOperation, EffectRef, SignatureAscription, SignatureItem, SignatureType, Stmt, TypeExpr,
    TypeParam,
};

pub(crate) fn hides_type(item: &SignatureItem) -> bool {
    matches!(
        item,
        SignatureItem::Type {
            definition: SignatureType::Abstract,
            ..
        } | SignatureItem::Type { opaque: true, .. }
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "structural conformance is an exhaustive canonical AST pair match"
)]
pub(crate) fn errors(implementation: &Stmt, contract: &SignatureItem) -> Vec<String> {
    match (implementation, contract) {
        (
            Stmt::Let {
                ty: Some(actual), ..
            },
            SignatureItem::Value { name, ty, .. },
        ) if !same_type(actual, ty) => {
            vec![format!("value `{name}` type does not match its signature")]
        }
        (
            Stmt::Function {
                type_params,
                parameters,
                return_type,
                effects,
                ..
            },
            SignatureItem::Function {
                name,
                type_params: expected_binders,
                parameters: expected_parameters,
                return_type: expected_return,
                effects: expected_effects,
                ..
            },
        ) => function_errors(
            name,
            FunctionShape {
                binders: type_params,
                parameters: parameters.iter().map(|parameter| parameter.ty.as_ref()),
                return_type: return_type.as_ref(),
                effects,
            },
            &ExpectedFunction {
                binders: expected_binders,
                parameters: expected_parameters,
                return_type: expected_return,
                effects: expected_effects,
            },
        ),
        (
            Stmt::Extern {
                parameters,
                return_type,
                ..
            },
            SignatureItem::Function {
                name,
                type_params,
                parameters: expected_parameters,
                return_type: expected_return,
                effects: expected_effects,
                ..
            },
        ) => function_errors(
            name,
            FunctionShape {
                binders: &[],
                parameters: parameters.iter().map(|parameter| Some(&parameter.ty)),
                return_type: return_type.as_ref(),
                effects: &[],
            },
            &ExpectedFunction {
                binders: type_params,
                parameters: expected_parameters,
                return_type: expected_return,
                effects: expected_effects,
            },
        ),
        (
            Stmt::Type {
                type_params, alias, ..
            },
            SignatureItem::Type {
                name,
                type_params: expected_binders,
                definition,
                ..
            },
        ) => type_errors(
            name,
            type_params,
            alias.as_ref(),
            expected_binders,
            definition,
        ),
        (
            Stmt::Effect {
                type_params,
                operations,
                ..
            },
            SignatureItem::Effect {
                name,
                type_params: expected_binders,
                operations: expected,
                ..
            },
        ) => effect_errors(name, type_params, operations, expected_binders, expected),
        (
            Stmt::Module { signature, .. },
            SignatureItem::Module {
                path,
                signature: expected,
                ..
            },
        ) if signature
            .as_ref()
            .is_none_or(|actual| !same_ascription(actual, expected)) =>
        {
            vec![format!(
                "nested module `{path}` does not match its signature ascription"
            )]
        }
        _ => Vec::new(),
    }
}

struct FunctionShape<'a, I> {
    binders: &'a [TypeParam],
    parameters: I,
    return_type: Option<&'a TypeExpr>,
    effects: &'a [EffectRef],
}

struct ExpectedFunction<'a> {
    binders: &'a [TypeParam],
    parameters: &'a [TypeExpr],
    return_type: &'a TypeExpr,
    effects: &'a [EffectRef],
}

fn function_errors<'a>(
    name: &str,
    actual: FunctionShape<'a, impl Iterator<Item = Option<&'a TypeExpr>>>,
    expected: &ExpectedFunction<'a>,
) -> Vec<String> {
    let actual_parameters = actual.parameters.collect::<Vec<_>>();
    let mut errors = Vec::new();
    if actual.binders.len() != expected.binders.len() {
        errors.push(format!(
            "function `{name}` generic arity does not match its signature"
        ));
    }
    if actual_parameters.len() != expected.parameters.len() {
        errors.push(format!(
            "function `{name}` parameter count does not match its signature"
        ));
    }
    if actual_parameters
        .iter()
        .zip(expected.parameters)
        .any(|(found, contract)| {
            found.is_some_and(|found| {
                !same_bound_type(found, contract, actual.binders, expected.binders)
            })
        })
    {
        errors.push(format!(
            "function `{name}` parameter types do not match its signature"
        ));
    }
    if actual.return_type.is_some_and(|found| {
        !same_bound_type(
            found,
            expected.return_type,
            actual.binders,
            expected.binders,
        )
    }) {
        errors.push(format!(
            "function `{name}` return type does not match its signature"
        ));
    }
    if !actual.effects.is_empty()
        && !same_effects(
            actual.effects,
            expected.effects,
            actual.binders,
            expected.binders,
        )
    {
        errors.push(format!(
            "function `{name}` effect row does not match its signature"
        ));
    }
    errors
}

fn type_errors(
    name: &str,
    binders: &[TypeParam],
    actual: Option<&TypeExpr>,
    expected_binders: &[TypeParam],
    definition: &SignatureType,
) -> Vec<String> {
    let mut errors = Vec::new();
    if binders.len() != expected_binders.len() {
        errors.push(format!(
            "type `{name}` generic arity does not match its signature"
        ));
    }
    if let SignatureType::Manifest(expected) = definition {
        if actual.is_none_or(|actual| !same_bound_type(actual, expected, binders, expected_binders))
        {
            errors.push(format!(
                "manifest type `{name}` does not match its signature"
            ));
        }
    }
    errors
}

fn effect_errors(
    name: &str,
    binders: &[TypeParam],
    actual: &[EffectOperation],
    expected_binders: &[TypeParam],
    expected: &[EffectOperation],
) -> Vec<String> {
    let matches = binders.len() == expected_binders.len()
        && actual.len() == expected.len()
        && expected.iter().all(|contract| {
            actual
                .iter()
                .find(|operation| operation.name == contract.name)
                .is_some_and(|operation| {
                    same_operation(operation, contract, binders, expected_binders)
                })
        });
    if matches {
        Vec::new()
    } else {
        vec![format!(
            "effect `{name}` operations do not match its signature"
        )]
    }
}

fn same_operation(
    left: &EffectOperation,
    right: &EffectOperation,
    left_binders: &[TypeParam],
    right_binders: &[TypeParam],
) -> bool {
    left.parameters.len() == right.parameters.len()
        && left
            .parameters
            .iter()
            .zip(&right.parameters)
            .all(|(left, right)| match (&left.ty, &right.ty) {
                (Some(left), Some(right)) => {
                    same_bound_type(left, right, left_binders, right_binders)
                }
                (None, None) => true,
                _ => false,
            })
        && normalize_written_type(&left.return_type, left_binders)
            == normalize_written_type(&right.return_type, right_binders)
}

fn same_effects(
    left: &[EffectRef],
    right: &[EffectRef],
    left_binders: &[TypeParam],
    right_binders: &[TypeParam],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name
                && left.type_args.len() == right.type_args.len()
                && left
                    .type_args
                    .iter()
                    .zip(&right.type_args)
                    .all(|(left, right)| same_bound_type(left, right, left_binders, right_binders))
        })
}

fn same_type(left: &TypeExpr, right: &TypeExpr) -> bool {
    same_bound_type(left, right, &[], &[])
}

fn same_bound_type(
    left: &TypeExpr,
    right: &TypeExpr,
    left_binders: &[TypeParam],
    right_binders: &[TypeParam],
) -> bool {
    same_type_name(&left.name, &right.name, left_binders, right_binders)
        && left.is_array == right.is_array
        && left.is_function == right.is_function
        && same_types(
            &left.generic_params,
            &right.generic_params,
            left_binders,
            right_binders,
        )
        && option_type_eq(
            left.array_element.as_deref(),
            right.array_element.as_deref(),
            left_binders,
            right_binders,
        )
        && same_types(
            &left.parameter_types,
            &right.parameter_types,
            left_binders,
            right_binders,
        )
        && option_type_eq(
            left.return_type.as_deref(),
            right.return_type.as_deref(),
            left_binders,
            right_binders,
        )
}

fn same_types(
    left: &[TypeExpr],
    right: &[TypeExpr],
    left_binders: &[TypeParam],
    right_binders: &[TypeParam],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| same_bound_type(left, right, left_binders, right_binders))
}

fn option_type_eq(
    left: Option<&TypeExpr>,
    right: Option<&TypeExpr>,
    left_binders: &[TypeParam],
    right_binders: &[TypeParam],
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => same_bound_type(left, right, left_binders, right_binders),
        (None, None) => true,
        _ => false,
    }
}

fn same_type_name(
    left: &str,
    right: &str,
    left_binders: &[TypeParam],
    right_binders: &[TypeParam],
) -> bool {
    let left_index = left_binders.iter().position(|binder| binder.name == left);
    let right_index = right_binders.iter().position(|binder| binder.name == right);
    match (left_index, right_index) {
        (Some(left), Some(right)) => left == right,
        (None, None) => left == right,
        _ => false,
    }
}

fn same_ascription(left: &SignatureAscription, right: &SignatureAscription) -> bool {
    left.path == right.path && left.allow_extra == right.allow_extra
}

fn normalize_written_type(value: &str, binders: &[TypeParam]) -> String {
    let mut normalized = value.to_string();
    for (index, binder) in binders.iter().enumerate() {
        normalized = normalized.replace(&binder.name, &format!("${index}"));
    }
    normalized
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}
