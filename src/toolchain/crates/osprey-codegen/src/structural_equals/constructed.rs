//! The type of an operand the checker published no type for. A constructor
//! expression (`Box { v: 1 }`) carries none of its own, yet the checker
//! accepted the comparison, so codegen must not reject it: the constructor's
//! declared fields are instantiated from the types of the expressions written
//! into them. Implements [BUILTIN-STRUCTURAL-EQUALS].

use crate::builder::Codegen;
use osprey_ast::{Expr, FieldAssignment};
use osprey_types::Type;
use std::collections::BTreeMap;

/// The concrete type `expr` has before it is generated, or `None` when
/// nothing short of generating it can tell.
pub(super) fn operand_type(cg: &Codegen, expr: &Expr) -> Option<Type> {
    let ty = match expr {
        Expr::TypeConstructor { name, fields, .. } => constructed_type(cg, name, fields),
        // A local's published type, else a payload-free constructor (`Open`),
        // whose type is its owner.
        Expr::Identifier(name) => cg
            .lookup(name)
            .and_then(|value| value.inferred_type)
            .or_else(|| constructed_type(cg, name, &[])),
        other => crate::expr::static_type(cg, other),
    }?;
    (!osprey_types::has_type_var(&ty)).then_some(ty)
}

/// A constructor's owner type with every type parameter bound by a field. A
/// declared generic field is `Var(i)` for the owner's `i`-th parameter
/// (`ProgramTypes::field_type` substitutes the same way), so matching each
/// declared field against its written expression's type binds the parameters.
/// A parameter no field mentions (`None` of `Option<T>`) leaves it open.
fn constructed_type(cg: &Codegen, name: &str, fields: &[FieldAssignment]) -> Option<Type> {
    let layout = cg.prog.ctors.get(name)?;
    let mut args = vec![None; layout.type_params.len()];
    for (field, declared) in &layout.fields {
        if let Some(actual) = written_type(cg, fields, field) {
            bind(declared, &actual, &mut args);
        }
    }
    let args: Option<Vec<Type>> = args.into_iter().collect();
    Some(Type::con(layout.owner.clone(), args?))
}

fn written_type(cg: &Codegen, fields: &[FieldAssignment], field: &str) -> Option<Type> {
    let written = fields.iter().find(|assignment| assignment.name == field)?;
    operand_type(cg, &written.value)
}

/// Bind the type variables `declared` mentions to the matching parts of
/// `actual`. The first binding of a variable wins; the checker already proved
/// every field consistent.
fn bind(declared: &Type, actual: &Type, args: &mut [Option<Type>]) {
    match declared {
        Type::Var(id) => bind_var(*id, actual, args),
        Type::Con { name, args: inner } => bind_con(name, inner, actual, args),
        Type::Record { fields, .. } => bind_record(fields, actual, args),
        _ => {}
    }
}

/// A named type binds through its arguments, when `actual` names the same type.
fn bind_con(name: &str, inner: &[Type], actual: &Type, args: &mut [Option<Type>]) {
    if let Type::Con {
        name: other,
        args: outer,
    } = actual
    {
        if name == other {
            bind_all(inner.iter().zip(outer), args);
        }
    }
}

/// A record binds through every field both rows name.
fn bind_record(fields: &BTreeMap<String, Type>, actual: &Type, args: &mut [Option<Type>]) {
    if let Type::Record {
        fields: written, ..
    } = actual
    {
        let pairs = fields
            .iter()
            .filter_map(|(field, d)| written.get(field).map(|a| (d, a)));
        bind_all(pairs, args);
    }
}

/// Bind parameter `id` to `actual` unless an earlier field bound it.
fn bind_var(id: u32, actual: &Type, args: &mut [Option<Type>]) {
    let slot = usize::try_from(id)
        .ok()
        .and_then(|index| args.get_mut(index));
    if let Some(slot @ None) = slot {
        *slot = Some(actual.clone());
    }
}

fn bind_all<'t>(pairs: impl Iterator<Item = (&'t Type, &'t Type)>, args: &mut [Option<Type>]) {
    for (declared, actual) in pairs {
        bind(declared, actual, args);
    }
}
