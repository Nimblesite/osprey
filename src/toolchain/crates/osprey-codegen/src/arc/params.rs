//! Owned parameters [GC-ARC-PERCEUS] (Perceus TR §2.2; the calling rule is
//! [MEM-OWNERSHIP-STATIC-PARAMETERS] in docs/specs/compiler/0018-MemoryManagement.md).
//!
//! A parameter is normally a borrow: the caller keeps the value alive for the
//! call. A function whose result can be one of its parameters would have to
//! count a reference to hand it back, so it takes ownership of that parameter
//! instead: the caller gives its reference up at the call, and the function
//! returns it or releases it.
use super::{Codegen, Entry, Need};
use crate::llty::{LType, Value};
use osprey_ast::{AstNode, Expr, Program, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// For each function that owns any, which of its parameters it owns.
pub(crate) type OwnedParameters = BTreeMap<String, Vec<bool>>;

/// Whether `e` can evaluate to the binding `name` itself.
fn yields(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Identifier(found) => found == name,
        Expr::Block {
            value: Some(tail), ..
        } => yields(tail, name),
        Expr::Match { arms, .. } => arms.iter().any(|arm| yields(&arm.body, name)),
        _ => false,
    }
}

/// Whether `e` can evaluate to a closure that captures the binding `name`: the
/// closure leaves the function, so it must own what it captured and the
/// parameter is taken by move [MEM-OWNERSHIP-STATIC-CLOSURES].
fn yields_closure_over(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Lambda { .. } => {
            let mut free = BTreeSet::new();
            osprey_ast::freevars::free_idents(e, &mut free);
            free.contains(name)
        }
        Expr::Block {
            value: Some(tail), ..
        } => yields_closure_over(tail, name),
        Expr::Match { arms, .. } => arms.iter().any(|arm| yields_closure_over(&arm.body, name)),
        _ => false,
    }
}

/// The collection builtins whose first argument is the source of a new version
/// [MEM-OWNERSHIP-STATIC-SHARING].
const DERIVING: [&str; 8] = [
    "listAppend",
    "listPrepend",
    "listReverse",
    "listSort",
    "listConcat",
    "mapSet",
    "mapRemove",
    "mapMerge",
];

/// Whether `e` is the binding `name` itself.
fn is_named(e: &Expr, name: &str) -> bool {
    matches!(e, Expr::Identifier(found) if found == name)
}

/// Whether the function body under `node` hands the binding `name` itself to a
/// value it builds: a field of a record or object, or the source, element, key
/// or value of a derived collection. Like a parameter it returns, such a parameter is taken by move
/// [MEM-OWNERSHIP-STATIC-PARAMETERS]. A nested closure is a function of its own.
fn stores(node: AstNode<'_>, name: &str) -> bool {
    let here = match node {
        AstNode::Expression(Expr::Lambda { .. }) => return false,
        AstNode::Expression(Expr::TypeConstructor { fields, .. } | Expr::Object(fields)) => {
            fields.iter().any(|field| is_named(&field.value, name))
        }
        // An updated copy takes the fields it keeps out of its base.
        AstNode::Expression(Expr::Update { record, fields }) => {
            record == name || fields.iter().any(|field| is_named(&field.value, name))
        }
        AstNode::Expression(Expr::Call {
            function,
            arguments,
            ..
        }) => {
            // Every argument is kept by the new version but a removed key.
            let kept = match named_callee(function) {
                Some(Expr::Identifier(callee)) if callee == "mapRemove" => 1,
                Some(Expr::Identifier(callee)) if DERIVING.contains(&callee.as_str()) => {
                    arguments.len()
                }
                _ => 0,
            };
            arguments
                .iter()
                .take(kept)
                .any(|argument| is_named(argument, name))
        }
        _ => false,
    };
    let mut below = false;
    node.for_each_child(|child| below = below || stores(child, name));
    here || below
}

/// Whether `e` is the binding `name` or a field read out of it.
fn rooted(e: &Expr, name: &str) -> bool {
    match e {
        Expr::FieldAccess { target, .. } => rooted(target, name),
        _ => is_named(e, name),
    }
}

/// Whether `e` can evaluate to a part of the binding `name`: a field of it, or
/// what a pattern binds out of it. A function that returns part of a parameter
/// takes that part out of it, so it owns the parameter and frees the rest
/// [MEM-OWNERSHIP-STATIC-PATHS]; `part_outlives_holder` proves the part
/// survives that free [MEM-OWNERSHIP-STATIC-MODEL].
fn yields_part(e: &Expr, name: &str) -> bool {
    match e {
        Expr::FieldAccess { target, .. } => rooted(target, name),
        Expr::Block {
            value: Some(tail), ..
        } => yields_part(tail, name),
        Expr::Match { value, arms } => arms.iter().any(|arm| {
            let bound = osprey_ast::freevars::pattern_bindings(&arm.pattern);
            let of_bound = |part: &String| yields(&arm.body, part) || yields_part(&arm.body, part);
            (rooted(value, name) && bound.iter().any(of_bound)) || yields_part(&arm.body, name)
        }),
        _ => false,
    }
}

/// Whether anything under `node` binds `name` again, hiding the parameter.
fn rebinds(node: AstNode<'_>, name: &str) -> bool {
    let here = match node {
        AstNode::Statement(Stmt::Let { name: bound, .. }) => bound == name,
        AstNode::Expression(Expr::Lambda { parameters, .. }) => {
            parameters.iter().any(|parameter| parameter.name == name)
        }
        AstNode::Expression(Expr::Match { arms, .. }) => arms.iter().any(|arm| {
            osprey_ast::freevars::pattern_bindings(&arm.pattern)
                .iter()
                .any(|bound| bound == name)
        }),
        _ => false,
    };
    let mut below = false;
    node.for_each_child(|child| below = below || rebinds(child, name));
    here || below
}

/// Collect every name used as a value rather than called: a function passed
/// around is called through a pointer that cannot say which parameters it owns.
fn value_uses(node: AstNode<'_>, out: &mut BTreeSet<String>) {
    let called = match node {
        AstNode::Expression(Expr::Call { function, .. }) => named_callee(function),
        _ => None,
    };
    match node {
        AstNode::Expression(Expr::Identifier(name)) => {
            let _ = out.insert(name.clone());
        }
        AstNode::Expression(Expr::Path(path)) => {
            let _ = out.insert(path.to_string());
        }
        _ => {}
    }
    node.for_each_child(|child| value_uses_except(child, called, out));
}

/// [`value_uses`] of `node`, apart from the identifier `called`: a callee, which
/// an explicit type application may wrap.
fn value_uses_except(node: AstNode<'_>, called: Option<&Expr>, out: &mut BTreeSet<String>) {
    match (node, called) {
        (AstNode::Expression(e), Some(name)) if std::ptr::eq(e, name) => {}
        (AstNode::Expression(Expr::TypeApply { .. }), Some(_)) => {
            node.for_each_child(|child| value_uses_except(child, called, out));
        }
        _ => value_uses(node, out),
    }
}

/// The identifier a call's callee names, through any explicit type application.
pub(super) fn named_callee(function: &Expr) -> Option<&Expr> {
    match function {
        Expr::Identifier(_) => Some(function),
        Expr::TypeApply { function, .. } => named_callee(function),
        _ => None,
    }
}

/// Decide which parameters each function of `program` owns. Only a plain
/// function of an executable qualifies: one that is emitted once, called only
/// by name, and that no host calls.
pub(crate) fn infer(cg: &mut Codegen, program: &Program) {
    if !cg.only_entry_is_external() {
        return;
    }
    let mut values = BTreeSet::new();
    for statement in &program.statements {
        value_uses(AstNode::Statement(statement), &mut values);
    }
    for statement in &program.statements {
        let Stmt::Function {
            name,
            parameters,
            body,
            ..
        } = statement
        else {
            continue;
        };
        if name == "main" || values.contains(name) {
            continue;
        }
        // A part that is a plain number is copied out, not taken.
        let managed = cg.fn_ret_ltype(name).is_some_and(LType::is_managed_ptr);
        let owned: Vec<bool> = parameters
            .iter()
            .map(|p| {
                (yields(body, &p.name)
                    || yields_closure_over(body, &p.name)
                    || (managed && yields_part(body, &p.name))
                    || stores(AstNode::Expression(body), &p.name))
                    && !rebinds(AstNode::Expression(body), &p.name)
            })
            .collect();
        if owned.contains(&true) {
            let _ = cg.ownership.owned_parameters.insert(name.clone(), owned);
        }
    }
}

/// The parameters `function` owns: an instantiation of a generic function owns
/// what the function it was made from owns.
fn owned_by<'a>(cg: &'a Codegen, function: &str) -> Option<&'a Vec<bool>> {
    let written = crate::monofn::source_name(function);
    cg.ownership.owned_parameters.get(written)
}

/// Whether `function` owns any of its parameters.
pub(crate) fn owns_a_parameter(cg: &Codegen, function: &str) -> bool {
    owned_by(cg, function).is_some()
}

pub(super) fn owns(cg: &Codegen, function: &str, index: usize) -> bool {
    let owned = owned_by(cg, function);
    owned
        .and_then(|owned| owned.get(index))
        .copied()
        .unwrap_or(false)
}

/// `function` begins with its parameter `index`, named `name`, bound to `v`:
/// when it owns that parameter, the ledger holds the reference the caller gave.
pub(crate) fn own_parameter(cg: &mut Codegen, function: &str, index: usize, name: &str, v: &Value) {
    if !owns(cg, function, index) || !super::managed(cg, v) {
        return;
    }
    let pointer = super::as_i8ptr(cg, v);
    let slot = cg.hoist_arc_slot();
    cg.emit(format!("store i8* {pointer}, i8** {slot}"));
    let entry = Entry {
        operand: v.operand.clone(),
        name: Some(name.to_owned()),
        binding_scope: cg.scope_id(),
        slot,
        pure_scalar: false,
        lent: false,
    };
    cg.arc.unscoped_parameters.push(name.to_owned());
    super::push_parent(cg, entry);
}

/// The function body's own block has opened: its owned parameters live in that
/// block's scope, like bindings made there, so last-use release and moves see
/// them.
pub(crate) fn scope_parameters(cg: &mut Codegen) {
    if cg.arc.frames.len() != 1 || cg.arc.unscoped_parameters.is_empty() {
        return;
    }
    let scope = cg.scope_id();
    let parameters = std::mem::take(&mut cg.arc.unscoped_parameters);
    for entry in cg.arc.frames.iter_mut().flatten() {
        if entry
            .name
            .as_ref()
            .is_some_and(|name| parameters.contains(name))
        {
            entry.binding_scope = scope;
        }
    }
}

/// `function` is about to be called with `arguments`: each one it owns is
/// handed over. A fresh value or a binding at its last use moves; anything
/// else needs a reference of its own to give. In the ledger model the gift is
/// a `move`, and `given_is_gone` proves the giver can't use it again
/// [MEM-OWNERSHIP-STATIC-MODEL].
pub(crate) fn give_arguments(cg: &mut Codegen, function: &str, arguments: &[Value]) {
    if !owns_a_parameter(cg, function) {
        return;
    }
    for (index, argument) in arguments.iter().enumerate() {
        let managed = owns(cg, function, index) && super::managed(cg, argument);
        if managed && !super::consume_into_store(cg, &argument.operand) {
            super::second_owner(cg, &argument.operand, None, Need::Given);
            super::retain_val(cg, argument);
        }
    }
}
