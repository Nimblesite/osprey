//! Redundant type-annotation detection.
//!
//! Implements [TYPE-ANNOTATION-REDUNDANT]. An annotation is redundant exactly
//! when erasing it leaves the solved program unchanged, which is a property of
//! inference and not of the annotation's spelling: the same
//! `string -> int -> string` is redundant on one function and load-bearing on
//! the next. The rule is therefore decided by re-running the inferrer over an
//! erased copy of the program and comparing what it published.

use std::collections::{BTreeMap, HashMap};

use osprey_ast::{Position, Program};

use crate::check::{check_program, infer_program};
use crate::redundant_sites::{erase, sites, Site, Slot};
use crate::ty::{Type, VarId};

/// The rule identifier this diagnostic is reported under, and the name a
/// per-rule severity setting will address it by ([TYPE-ANNOTATION-REDUNDANT]).
pub const REDUNDANT_ANNOTATION: &str = "redundant-annotation";

/// A diagnostic that is true of the program but is not a reason to reject it.
///
/// Warnings are reported alongside [`TypeError`](crate::TypeError)s and change
/// no exit code and no generated code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeWarning {
    /// Human-readable text, already naming the offending declaration.
    pub message: String,
    /// Where the annotated declaration was written, when recorded.
    pub position: Option<Position>,
    /// The rule that raised it.
    pub rule: &'static str,
}

/// Every type the inferrer publishes, under a stable label.
///
/// Function signatures are keyed by name; bindings, lambdas and list literals
/// are keyed by source position, which an erasure never moves. Comparing the
/// whole map is what makes the rule honest: an annotation that changed only a
/// `let`'s type is still an annotation that changed something.
type Published = BTreeMap<String, Type>;

/// Every redundant annotation in `program`, in source order.
///
/// An ill-typed program yields none: its inferred types are the checker's
/// best effort at a program that does not typecheck, so no conclusion drawn
/// from comparing them would be trustworthy.
#[must_use]
pub fn redundant_annotations(program: &Program) -> Vec<TypeWarning> {
    if !check_program(program).is_empty() {
        return Vec::new();
    }
    let written = sites(program);
    if written.is_empty() {
        return Vec::new();
    }
    let baseline = published(program);
    written
        .iter()
        .enumerate()
        .filter(|(index, _)| preserves(program, &baseline, *index))
        .map(|(_, site)| warn(site))
        .collect()
}

/// Whether erasing this annotation leaves `program` typechecking
/// and publishing the same types, up to renaming of type variables.
///
/// The published types are compared first because they are what usually
/// differs, and re-checking is a second full solve this can then skip.
fn preserves(program: &Program, baseline: &Published, index: usize) -> bool {
    let candidate = erase(program, &mut |site| site != index);
    same_types(baseline, &published(&candidate)) && check_program(&candidate).is_empty()
}

/// Everything the inferrer resolved, labelled so two runs compare entry by
/// entry.
fn published(program: &Program) -> Published {
    let types = infer_program(program);
    let functions = types.functions.iter().map(|(name, (params, ret))| {
        let signature = Type::Fun {
            params: params.clone(),
            ret: Box::new(ret.clone()),
        };
        (format!("fn {name}"), signature)
    });
    let mut published: Published = functions
        .chain(sited("let", &types.lets))
        .chain(sited("lambda", &types.lambdas))
        .chain(sited("list", &types.lists))
        .collect();
    for (function, binders) in &types.declared_params {
        for (name, ty) in binders {
            let _ = published.insert(format!("binder {function} {name}"), ty.clone());
        }
    }
    for (position, site) in &types.performs {
        publish_operation(&mut published, &format!("perform {position:?}"), &site.op, &site.effect_args);
    }
    for (position, site) in &types.handler_ops {
        for (operation, op) in &site.ops {
            publish_operation(&mut published, &format!("handler {position:?} {operation}"), op, &site.effect_args);
        }
    }
    published
}

fn publish_operation(published: &mut Published, label: &str, operation: &crate::info::OpType, arguments: &[Type]) {
    let _ = published.insert(label.to_owned(), Type::fun(operation.params.clone(), operation.ret.clone()));
    for (index, argument) in arguments.iter().enumerate() {
        let _ = published.insert(format!("{label} argument {index}"), argument.clone());
    }
}

/// Label one position-keyed table so its entries join the same flat map.
fn sited<'a>(
    kind: &'a str,
    table: &'a HashMap<(u32, u32), Type>,
) -> impl Iterator<Item = (String, Type)> + 'a {
    table
        .iter()
        .map(move |((line, column), ty)| (format!("{kind} {line}:{column}"), ty.clone()))
}

/// The diagnostic for one redundant annotation.
///
/// The reported type is the written one: redundancy *means* inference derives
/// exactly that, so printing it tells the reader the deletion is a no-op.
fn warn(site: &Site) -> TypeWarning {
    let written = &site.written;
    let message = match &site.slot {
        Slot::Param {
            owner, parameter, ..
        } => format!(
            "redundant type annotation on parameter `{parameter}` of `{owner}`: inference derives `{written}` without it"
        ),
        Slot::Return { owner } => format!(
            "redundant return type annotation on `{owner}`: inference derives `{written}` without it"
        ),
        Slot::Binding { name } => format!(
            "redundant type annotation on `{name}`: inference derives `{written}` without it"
        ),
    };
    TypeWarning {
        message: osprey_ast::symbol::demangle_message(&message).into_owned(),
        position: site.position,
        rule: REDUNDANT_ANNOTATION,
    }
}

/// Whether two runs published the same types up to a consistent renaming of
/// inference variables.
///
/// The renaming is global rather than per entry: a shared variable is real
/// sharing between two signatures, and an erasure that breaks it changed the
/// program's types even though each signature alone still looks the same.
fn same_types(left: &Published, right: &Published) -> bool {
    let mut renaming = Renaming::default();
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|((left_label, left_ty), (right_label, right_ty))| {
                left_label == right_label && equivalent(left_ty, right_ty, &mut renaming)
            })
}

/// A partial bijection between the two runs' inference variables.
#[derive(Default)]
struct Renaming {
    /// Left variable → the right variable it was first paired with.
    forward: HashMap<VarId, VarId>,
    /// Right variable → the left variable it was first paired with.
    backward: HashMap<VarId, VarId>,
}

impl Renaming {
    /// Pair `left` with `right`, failing if either is already paired elsewhere.
    fn pair(&mut self, left: VarId, right: VarId) -> bool {
        matches(&mut self.forward, left, right) && matches(&mut self.backward, right, left)
    }
}

/// Whether `key` already maps to `value` in `map`, recording it if it is new.
fn matches(map: &mut HashMap<VarId, VarId>, key: VarId, value: VarId) -> bool {
    match map.insert(key, value) {
        Some(existing) => existing == value,
        None => true,
    }
}

/// Whether two types are equal up to the variable renaming built so far.
fn equivalent(left: &Type, right: &Type, renaming: &mut Renaming) -> bool {
    match (left, right) {
        (Type::Var(l), Type::Var(r)) => renaming.pair(*l, *r),
        (Type::Con { name: l, args: la }, Type::Con { name: r, args: ra })
        | (
            Type::Union {
                name: l,
                variants: la,
            },
            Type::Union {
                name: r,
                variants: ra,
            },
        ) => l == r && all_equivalent(la, ra, renaming),
        (
            Type::Fun {
                params: lp,
                ret: lr,
            },
            Type::Fun {
                params: rp,
                ret: rr,
            },
        ) => all_equivalent(lp, rp, renaming) && equivalent(lr, rr, renaming),
        (
            Type::Record {
                name: l,
                fields: lf,
            },
            Type::Record {
                name: r,
                fields: rf,
            },
        ) => l == r && lf.keys().eq(rf.keys()) && all_equivalent_values(lf, rf, renaming),
        _ => false,
    }
}

/// Whether two type sequences are pairwise equivalent under one renaming.
fn all_equivalent(left: &[Type], right: &[Type], renaming: &mut Renaming) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(l, r)| equivalent(l, r, renaming))
}

/// Whether two records' field types are pairwise equivalent. Fields are keyed
/// in a `BTreeMap`, so iteration already aligns them by name.
fn all_equivalent_values(
    left: &BTreeMap<String, Type>,
    right: &BTreeMap<String, Type>,
    renaming: &mut Renaming,
) -> bool {
    left.values()
        .zip(right.values())
        .all(|(l, r)| equivalent(l, r, renaming))
}
