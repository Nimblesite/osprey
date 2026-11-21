//! ML pattern lowering: one canonical [`Pattern`] per surface form, shared
//! with the Default flavor through [`crate::desugar`] ([FLAVOR-IR-EQUIV]).
use super::{lower_string, MlPattern, Pattern};
use crate::ml::cst::MlBinder;
use osprey_ast::Expr;

pub(in crate::ml) fn lower_pattern(pattern: MlPattern) -> Pattern {
    match pattern {
        MlPattern::Wildcard => Pattern::Wildcard,
        MlPattern::Int(n) => Pattern::Literal(Box::new(Expr::Integer(n))),
        MlPattern::Float(x) => Pattern::Literal(Box::new(Expr::Float(x))),
        MlPattern::Str(raw) => Pattern::Literal(Box::new(lower_string(&raw, None))),
        MlPattern::Bool(b) => Pattern::Literal(Box::new(Expr::Bool(b))),
        MlPattern::Bind(name) => Pattern::Binding(crate::ml::binding_ranges::pattern(name)),
        MlPattern::Structural { fields, open } => Pattern::Structural {
            fields: fields.into_iter().map(lower_row_slot).collect(),
            open,
        },
        MlPattern::Tuple(elements) => {
            osprey_ast::tuple_pattern(elements.into_iter().map(lower_pattern).collect())
        }
        MlPattern::Labelled { name, slots } => crate::desugar::labelled_ctor_pattern(
            name,
            slots
                .into_iter()
                .map(|(field, p)| (field, lower_pattern(p)))
                .collect(),
        ),
        MlPattern::Ctor { name, slots } => lower_ctor(name, slots),
        MlPattern::List { elements, rest } => Pattern::List {
            elements: elements.into_iter().map(lower_pattern).collect(),
            rest: rest.map(crate::ml::binding_ranges::pattern),
        },
    }
}

/// A row slot: a bare field binds under its own name ([PATTERN-NESTED]).
fn lower_row_slot((field, pattern): (MlBinder, Option<MlPattern>)) -> (String, Pattern) {
    if let Some(pattern) = pattern {
        return (field.name, lower_pattern(pattern));
    }
    let name = crate::ml::binding_ranges::pattern(field);
    (name.clone(), Pattern::Binding(name))
}

/// `Ctor a b` keeps the binder-only shape both flavors share; a slot that
/// tests its value lowers as Default's `Ctor(p, …)` does ([PATTERN-NESTED]).
fn lower_ctor(name: String, slots: Vec<MlPattern>) -> Pattern {
    let flat = slots
        .iter()
        .all(|slot| matches!(slot, MlPattern::Bind(_) | MlPattern::Wildcard));
    if flat {
        let binders = slots
            .into_iter()
            .map(|slot| match slot {
                MlPattern::Bind(binder) => crate::ml::binding_ranges::pattern(binder),
                _ => "_".to_owned(),
            })
            .collect();
        return crate::desugar::ctor_pattern(name, binders);
    }
    let patterns = slots.into_iter().map(lower_pattern).collect();
    crate::desugar::positional_ctor_pattern(name, patterns)
}
