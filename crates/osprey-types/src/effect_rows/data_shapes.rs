//! Values whose type can never hold a function.
//!
//! The effect proof tracks where a callable came from so a later call can be
//! charged its row. A value whose type, all the way down, holds no function,
//! fiber, channel, iterator or `any` can never be called and never resumes a
//! latent effect, so that tracking only costs: the proof kernel's terms and
//! environments carried hundreds of parameter projections per return and made
//! the Workbench's fixed point the slowest step of its build. Such a value
//! keeps exactly what a scalar keeps ([`super::value_shapes::value_shape`]):
//! its field names, which still let a method call fall back to a free
//! function.
use super::{BTreeSet, HashMap, Value};
use crate::check::CtorInfo;
use crate::ty::{names, Type};

/// Implements [EFFECTS-PROVENANCE]: declared names cannot regain builtin identity.
#[derive(Clone, Default)]
pub(crate) struct DataTypes {
    fields: HashMap<String, BTreeSet<String>>,
    declared: BTreeSet<String>,
}

/// Every declared type no value of which can hold a function, mapped to its
/// field names (none for a union, whose values have no fields of their own).
/// A generic type is never included: its parameter may be a function. A
/// recursive type is included unless some field reaches a function, so this
/// is the greatest set closed under "every field is function-free".
pub(crate) fn data_types(ctors: &HashMap<String, CtorInfo>) -> DataTypes {
    let owners = owners(ctors);
    let declared = owners.keys().cloned().collect();
    let data = closed_owners(&owners, &declared);
    let fields = owners
        .into_iter()
        .filter(|(name, _)| data.contains(name))
        .map(|(name, owner)| (name, owner.field_names))
        .collect();
    DataTypes { fields, declared }
}

fn candidates(owners: &HashMap<String, Owner>) -> BTreeSet<&str> {
    owners
        .iter()
        .filter(|(_, owner)| !owner.generic)
        .map(|(name, _)| name.as_str())
        .collect()
}

fn closed_owners(owners: &HashMap<String, Owner>, declared: &BTreeSet<String>) -> BTreeSet<String> {
    let mut data = candidates(owners);
    loop {
        let kept = closed_subset(owners, &data, declared);
        if kept.len() == data.len() {
            break;
        }
        data = kept;
    }
    data.into_iter().map(str::to_owned).collect()
}

fn closed_subset<'a>(
    owners: &HashMap<String, Owner>,
    data: &BTreeSet<&'a str>,
    declared: &BTreeSet<String>,
) -> BTreeSet<&'a str> {
    data.iter()
        .copied()
        .filter(|name| {
            owners
                .get(*name)
                .is_some_and(|owner| owner.closed(data, declared))
        })
        .collect()
}

/// The shape of a value of type `ty` when that type can never hold a
/// function, given the declared [`data_types`]; `None` otherwise.
pub(super) fn data_shape(ty: &Type, data: &DataTypes) -> Option<Value> {
    let known = |name: &str| {
        data.declared
            .contains(name)
            .then(|| data.fields.contains_key(name))
    };
    let field_names = match ty {
        _ if !function_free(ty, &known) => return None,
        Type::Record { fields, .. } => fields.keys().cloned().collect(),
        Type::Con { name, .. } => data.fields.get(name).cloned().unwrap_or_default(),
        _ => BTreeSet::new(),
    };
    Some(Value {
        field_names: Some(field_names),
        ..Value::default()
    })
}

/// One declared type: every field type of every constructor, the field names
/// of a record, and whether any constructor takes type parameters.
#[derive(Default)]
struct Owner {
    fields: Vec<Type>,
    field_names: BTreeSet<String>,
    generic: bool,
}

impl Owner {
    fn closed(&self, data: &BTreeSet<&str>, declared: &BTreeSet<String>) -> bool {
        let known = |name: &str| declared.contains(name).then(|| data.contains(name));
        self.fields.iter().all(|ty| function_free(ty, &known))
    }
}

fn owners(ctors: &HashMap<String, CtorInfo>) -> HashMap<String, Owner> {
    let mut owners: HashMap<String, Owner> = HashMap::new();
    let unbound = HashMap::new();
    for info in ctors.values() {
        let owner = owners.entry(info.owner.clone()).or_default();
        owner.generic |= !info.type_params.is_empty();
        for (name, written) in &info.fields {
            owner
                .fields
                .push(crate::convert::type_name_to_type(written, &unbound));
            if info.owner_is_record {
                let _ = owner.field_names.insert(name.clone());
            }
        }
    }
    owners
}

/// Whether no value of `ty` can hold a function, a fiber, a channel, an
/// iterator or an `any`, reading a named type through `known`.
fn function_free(ty: &Type, known: &impl Fn(&str) -> Option<bool>) -> bool {
    match ty {
        Type::Con { name, args } => constructor_free(name, args, known),
        Type::Record { fields, .. } => fields.values().all(|field| function_free(field, known)),
        Type::Union { variants, .. } => {
            variants.iter().all(|variant| function_free(variant, known))
        }
        Type::Fun { .. } | Type::Var(_) => false,
    }
}

fn constructor_free(name: &str, args: &[Type], known: &impl Fn(&str) -> Option<bool>) -> bool {
    known(name).map_or_else(|| builtin_data(name), |closed| args.is_empty() && closed)
        && args.iter().all(|arg| function_free(arg, known))
}

fn builtin_data(name: &str) -> bool {
    matches!(
        name,
        names::INT
            | names::FLOAT
            | names::BOOL
            | names::STRING
            | names::UNIT
            | names::LIST
            | names::MAP
            | names::RESULT
    )
}

#[cfg(test)]
#[path = "data_shapes_tests.rs"]
mod tests;
