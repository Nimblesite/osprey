//! Effect rows written on function-typed fields: `edit: fn(string) -> A ![]`
//! bounds every function a construction stores in the field, and a call
//! through the field — projected, pattern-bound or called as a method —
//! performs exactly the written row, whatever provenance the stored value kept
//! on its way there. Implements [EFFECTS-FIELD-ROWS].
use super::{
    requirement_name, Analyzer, Callable, CallableEnv, Expr, FieldAssignment, HashMap, Index,
    KnownCallable, Requirement, Summary, TypeError, Value,
};
use osprey_ast::{EffectRef, TypeField, TypeParam};
use std::collections::BTreeMap;

/// Written field rows keyed by constructor so a projection finds its owner.
#[derive(Default)]
pub(super) struct FieldRows<'a> {
    written: HashMap<String, Vec<(&'a str, &'a TypeField)>>,
    parameters: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct FieldInstance {
    pub(super) name: String,
    pub(super) arguments: Vec<String>,
}

/// A violated field row, keyed by construction site and field so the last
/// analysis of each site — the one over converged rows — decides it.
pub(crate) type FieldRowViolations = BTreeMap<(usize, String, String), TypeError>;

/// Which declaration a record-typed site belongs to. Only a named declaration
/// owns a written row, so only its row may stand in for lost provenance; an
/// anonymous record declares nothing to borrow. An unresolved receiver keeps
/// its tracked value provenance without borrowing another declaration's row.
pub(super) enum FieldOwner {
    Named(FieldInstance),
    Anonymous,
    Unknown,
}

impl<'a> FieldRows<'a> {
    pub(super) fn register_parameters(&mut self, name: &str, parameters: &[TypeParam]) {
        let _ = self.parameters.insert(
            name.to_owned(),
            parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect(),
        );
    }

    pub(super) fn collect(
        &mut self,
        constructor: &'a str,
        parameters: &[TypeParam],
        fields: &'a [TypeField],
    ) {
        self.register_parameters(constructor, parameters);
        for field in fields {
            if field.effects.is_some() {
                self.written
                    .entry(field.name.clone())
                    .or_default()
                    .push((constructor, field));
            }
        }
    }

    fn of(&self, constructor: &str, field: &str) -> Option<&'a [EffectRef]> {
        self.written
            .get(field)?
            .iter()
            .find(|(owner, _)| *owner == constructor)
            .and_then(|(_, declared)| declared.effects.as_deref())
    }
}

impl Index<'_> {
    fn effect_argument_key(&self, ty: &crate::ty::Type) -> String {
        crate::ty::effect_instance_key(ty, &|name| {
            self.field_rows
                .parameters
                .get(name)
                .is_some_and(Vec::is_empty)
        })
    }

    /// What a call through `field` of the record `owner` invokes: the row that
    /// declaration writes stands in for the provenance the value lost. A
    /// record that declares no row has nothing to stand in — its stored value
    /// must prove its own effects.
    pub(super) fn owned_field_value(
        &self,
        owner: &FieldInstance,
        field: &str,
        projected: Option<Value>,
    ) -> Option<Value> {
        match self
            .field_rows
            .of(&owner.name, field)
            .and_then(|row| self.row_summary(row, owner).map(Self::known_value))
        {
            Some(trusted) => Some(trusted_row_value(trusted, projected)),
            None => projected,
        }
    }

    fn known_value(summary: Summary) -> Value {
        Value {
            callable: Some(Callable::Known(Box::new(KnownCallable {
                parameters: Vec::new(),
                summary,
                returned: None,
            }))),
            ..Value::default()
        }
    }

    /// Every operation the row's effects declare. `None` when an entry names
    /// no declared effect, or a generic one without its instance.
    fn row_summary(&self, row: &[EffectRef], owner: &FieldInstance) -> Option<Summary> {
        let parameters = self.field_rows.parameters.get(&owner.name)?;
        if parameters.len() != owner.arguments.len() {
            return None;
        }
        let bindings = parameters
            .iter()
            .cloned()
            .zip(owner.arguments.iter().cloned())
            .collect();
        let mut summary = Summary::default();
        for effect in row {
            let arguments: Vec<_> = self
                .row_arguments(effect)?
                .iter()
                .map(|argument| super::specialize_argument(argument, &bindings))
                .collect();
            for operation in self.effect_operations.get(&effect.name)? {
                let _ = summary.required.insert(Requirement::new(
                    &effect.name,
                    operation,
                    arguments.clone(),
                ));
            }
        }
        Some(summary)
    }

    fn row_arguments(&self, effect: &EffectRef) -> Option<Vec<String>> {
        let arity = *self.effects.get(&effect.name)?;
        (effect.type_args.len() == arity).then(|| {
            effect
                .type_args
                .iter()
                .map(|argument| crate::convert::type_expr_to_type(argument, &HashMap::new()))
                .map(|argument| self.effect_argument_key(&argument))
                .collect()
        })
    }

    /// A row that cannot bound its field: on a field that is not a function,
    /// or naming an effect that is not declared or is generic without its
    /// instance.
    pub(super) fn field_row_errors(&self) -> Vec<TypeError> {
        let mut errors = Vec::new();
        for (constructor, field) in self.field_rows.written.values().flatten() {
            let row = field.effects.as_deref().unwrap_or_default();
            let position = row.first().and_then(|effect| effect.position);
            let ty = crate::convert::type_name_to_type(&field.ty, &HashMap::new());
            if !matches!(ty, crate::ty::Type::Fun { .. }) {
                errors.push(declaration_error(
                    constructor,
                    field,
                    "an effect row needs a function type",
                    position,
                ));
            } else if row
                .iter()
                .any(|effect| self.row_arguments(effect).is_none())
            {
                errors.push(declaration_error(
                    constructor,
                    field,
                    "its effect row names an undeclared effect or a generic one without its type arguments",
                    position,
                ));
            }
        }
        errors
    }
}

/// The row as the call a field performs, plus the concrete closures the stored
/// value still holds. A value whose only callable was lost contributes nothing
/// further, so the row is exactly its contract.
fn trusted_row_value(trusted: Value, projected: Option<Value>) -> Value {
    let Some(mut projected) = projected else {
        return trusted;
    };
    let Some(stored) = stored_callables(projected.callable.as_ref()) else {
        return trusted;
    };
    projected.callable = Some(stored);
    projected.union(trusted);
    projected
}

/// The concrete closures a value still holds, joined where the value itself
/// was a join. `None` when nothing is a stored closure: a lost callable and a
/// symbolic parameter are both what the written row stands in for, so keeping
/// them would reject the call that row exists to prove.
fn stored_callables(callable: Option<&Callable>) -> Option<Callable> {
    let mut kept = Vec::new();
    collect_stored(callable?, &mut kept);
    kept.sort();
    kept.dedup();
    match kept.len() {
        1 => kept.first().cloned(),
        0 => None,
        _ => Some(Callable::Alternatives(kept)),
    }
}

fn collect_stored(callable: &Callable, kept: &mut Vec<Callable>) {
    match callable {
        Callable::Alternatives(choices) => {
            for choice in choices {
                collect_stored(choice, kept);
            }
        }
        Callable::Known(_) => kept.push(callable.clone()),
        Callable::Parameter { .. } | Callable::Unknown => {}
    }
}

fn declaration_error(
    constructor: &str,
    field: &TypeField,
    reason: &str,
    position: Option<osprey_ast::Position>,
) -> TypeError {
    TypeError::new(format!(
        "field `{}` of `{constructor}`: {reason}",
        field.name
    ))
    .with_pos(position)
}

impl Analyzer<'_> {
    pub(super) fn construction_owner(&self, expression: &Expr, name: &str) -> FieldInstance {
        let types = self.instances.expression_types.borrow();
        let arguments = match types.get(&super::expression_site(expression)) {
            Some(crate::ty::Type::Con { args, .. }) => args
                .iter()
                .map(|argument| self.index.effect_argument_key(argument))
                .collect(),
            _ => Vec::new(),
        };
        FieldInstance {
            name: name.to_owned(),
            arguments,
        }
    }

    pub(super) fn project_owned_field(&self, value: Value, field: &str) -> Option<Value> {
        let owner = value.field_owner.clone();
        let projected = super::project_field(value, field);
        match owner {
            Some(owner) => self.index.owned_field_value(&owner, field, projected),
            None => projected,
        }
    }

    /// Generic updates owe every declaration selected by an actual call. Keep
    /// the resolved types, since effect display keys erase generic owners.
    pub(super) fn updated_records(
        &self,
        expression: &Expr,
    ) -> std::collections::BTreeSet<FieldInstance> {
        let types = self.instances.expression_types.borrow();
        let Some(ty) = types.get(&super::expression_site(expression)) else {
            return std::collections::BTreeSet::default();
        };
        std::iter::once(ty.clone())
            .chain(
                self.instances
                    .instantiation_types
                    .values()
                    .map(|bindings| crate::env::subst_vars(ty, bindings)),
            )
            .filter_map(|ty| match self.owner_of_type(Some(&ty)) {
                FieldOwner::Named(name) => Some(name),
                FieldOwner::Anonymous | FieldOwner::Unknown => None,
            })
            .collect()
    }

    /// The declaration a field projection belongs to, from the receiver's
    /// resolved type at the site.
    pub(super) fn field_owner(&self, receiver: &Expr) -> FieldOwner {
        if let Expr::TypeConstructor { name, .. } = receiver {
            if self.index.constructors.contains_key(name) {
                return FieldOwner::Named(self.construction_owner(receiver, name));
            }
        }
        let types = self.instances.expression_types.borrow();
        self.owner_of_type(types.get(&super::expression_site(receiver)))
    }

    /// A construction writes into the declaration its own name selects, and an
    /// update writes into the declaration of its base binding's type. A record
    /// type with no name is the anonymous structural form: no declaration.
    pub(super) fn owner_of_type(&self, ty: Option<&crate::ty::Type>) -> FieldOwner {
        match ty {
            Some(crate::ty::Type::Record { name, .. }) if name.is_empty() => FieldOwner::Anonymous,
            Some(crate::ty::Type::Record { name, .. }) => FieldOwner::Named(FieldInstance {
                name: name.clone(),
                arguments: Vec::new(),
            }),
            // A declared record is also named nominally, as in an annotated
            // parameter; the constructor table names its declaration.
            Some(crate::ty::Type::Con { name, args })
                if self.index.field_rows.parameters.contains_key(name) =>
            {
                FieldOwner::Named(FieldInstance {
                    name: name.clone(),
                    arguments: args
                        .iter()
                        .map(|argument| self.index.effect_argument_key(argument))
                        .collect(),
                })
            }
            _ => FieldOwner::Unknown,
        }
    }

    /// A call through `field` of `owner`: only the owner's own declaration may
    /// stand in for provenance the value lost.
    pub(super) fn owned_field_value(
        &self,
        owner: &FieldOwner,
        field: &str,
        projected: Option<Value>,
    ) -> Option<Value> {
        match owner {
            FieldOwner::Named(owner) => self.index.owned_field_value(owner, field, projected),
            FieldOwner::Anonymous | FieldOwner::Unknown => projected,
        }
    }

    /// Hold each function stored in a row-declared field to that row.
    pub(super) fn check_field_rows(
        &self,
        site: usize,
        constructor: &FieldInstance,
        fields: &[FieldAssignment],
        scope: &[String],
        env: &CallableEnv,
    ) {
        for field in fields {
            let Some(row) = self.index.field_rows.of(&constructor.name, &field.name) else {
                continue;
            };
            let violation = self.field_row_violation(constructor, field, row, scope, env);
            let key = (
                site,
                format!("{}<{}>", constructor.name, constructor.arguments.join(", ")),
                field.name.clone(),
            );
            let mut violations = self.instances.field_row_violations.borrow_mut();
            let _ = match violation {
                Some(error) => violations.insert(key, error),
                None => violations.remove(&key),
            };
        }
    }

    fn field_row_violation(
        &self,
        constructor: &FieldInstance,
        field: &FieldAssignment,
        row: &[EffectRef],
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<TypeError> {
        let stored = self
            .callable(&field.value, scope, env)
            .map(|callee| self.invoke_with_values(callee, &[]));
        let reason = match stored {
            None => Some("the stored function's effects cannot be proven".to_owned()),
            Some(summary) => match self.index.row_summary(row, constructor) {
                Some(allowed) => outside_row(&summary, &allowed),
                None => Some("the field's generic effect instance cannot be proven".to_owned()),
            },
        }?;
        // A stored lambda carries its own position; anything else has none.
        let position = match &field.value {
            Expr::Lambda { position, .. } => *position,
            _ => None,
        };
        Some(
            TypeError::new(format!(
                "field `{}` of `{}` declares effect row {}, but {reason}",
                field.name,
                constructor.name,
                spelled(row)
            ))
            .with_pos(position),
        )
    }
}

/// Why `summary` exceeds `row`, or `None` when the row admits it.
fn outside_row(summary: &Summary, row: &Summary) -> Option<String> {
    let undeclared: Vec<_> = summary
        .required
        .iter()
        .filter(|requirement| {
            !row.required.iter().any(|allowed| {
                allowed.effect == requirement.effect && allowed.arguments == requirement.arguments
            })
        })
        .map(requirement_name)
        .collect();
    if !undeclared.is_empty() {
        return Some(format!(
            "the stored function performs {}",
            undeclared.join(", ")
        ));
    }
    if row.required.is_empty() && !summary.runtime_builtins.is_empty() {
        let builtins: Vec<_> = summary.runtime_builtins.iter().cloned().collect();
        return Some(format!("the stored function uses {}", builtins.join(", ")));
    }
    (!summary.parameter_uses.is_empty() || summary.unresolved_dynamic_call)
        .then(|| "the stored function calls a function whose effects cannot be proven".to_owned())
}

fn spelled(row: &[EffectRef]) -> String {
    let names: Vec<_> = row.iter().map(|effect| effect.name.as_str()).collect();
    format!("![{}]", names.join(", "))
}
