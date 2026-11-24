//! Lent closures [MEM-OWNERSHIP-STATIC-CLOSURES]
//! (docs/specs/compiler/0018-MemoryManagement.md). Under `--memory=ownership`
//! a closure written as an argument of a call to an Osprey function borrows
//! what it captures. Its cell lives until the calling statement ends, the
//! captured bindings outlive that statement, and a checked callee keeps only a
//! parameter it owns, so a cell passed for any other parameter owns none of its
//! captures and frees none.
use super::Codegen;
use crate::llty::Value;
use osprey_ast::{AstNode, Expr, Position};

/// Whether a call names an Osprey function or a local function value: code
/// this compilation checks, which keeps only the parameters it owns.
fn checked_callee(cg: &Codegen, function: &Expr) -> bool {
    match function {
        Expr::Identifier(name) => cg.fn_params.contains_key(name) || cg.lookup(name).is_some(),
        Expr::TypeApply { function, .. } => checked_callee(cg, function),
        _ => false,
    }
}

/// `argument` is passed to a checked callee: a closure literal lends, and so do
/// a list literal and every closure literal in it.
fn note_argument(cg: &mut Codegen, argument: &Expr) {
    match argument {
        Expr::Lambda {
            position: Some(at), ..
        } => {
            let _ = cg.ownership.lent_closures.insert((at.line, at.column));
        }
        Expr::List(items, Some(at)) => {
            let _ = cg.ownership.lent_lists.insert((at.line, at.column));
            for item in items
                .iter()
                .filter(|item| matches!(item, Expr::Lambda { .. }))
            {
                note_argument(cg, item);
            }
        }
        _ => {}
    }
}

/// The arguments of a call that the callee only borrows. A callee that owns a
/// parameter keeps what it is given, so that argument owns what it holds; one
/// called with named arguments is taken to own each of them.
fn borrowed_arguments<'a>(
    cg: &Codegen,
    function: &Expr,
    arguments: &'a [Expr],
    named: &'a [osprey_ast::NamedArgument],
) -> Vec<&'a Expr> {
    let callee = match super::params::named_callee(function) {
        Some(Expr::Identifier(name)) => name.as_str(),
        _ => "",
    };
    let positional = arguments
        .iter()
        .enumerate()
        .filter(|(index, _)| !super::params::owns(cg, callee, *index))
        .map(|(_, argument)| argument);
    let named = named
        .iter()
        .filter(|_| !super::params::owns_a_parameter(cg, callee))
        .map(|argument| &argument.value);
    positional.chain(named).collect()
}

/// Note every closure literal under `node` that is an argument of a call to a
/// checked callee.
fn note(cg: &mut Codegen, node: AstNode<'_>) {
    if let AstNode::Expression(Expr::Call {
        function,
        arguments,
        named_arguments,
    }) = node
    {
        if checked_callee(cg, function) {
            for argument in borrowed_arguments(cg, function, arguments, named_arguments) {
                note_argument(cg, argument);
            }
        }
    }
    node.for_each_child(|child| note(cg, child));
}

/// `value` is about to be lowered: find the closures in it that only lend.
pub(super) fn note_lent_closures(cg: &mut Codegen, value: &Expr) {
    if cg.ownership.on {
        note(cg, AstNode::Expression(value));
    }
}

/// The binding a place is read from: `a` for `a` and for `a.b.c`.
fn place_root(value: &Expr) -> Option<&str> {
    match value {
        Expr::Identifier(name) => Some(name),
        Expr::FieldAccess { target, .. } => place_root(target),
        _ => None,
    }
}

/// `stmt` binds a list literal whose elements are all bindings or fields of
/// them, and one of those bindings keeps its value: the list holds every
/// element without owning it, and borrows from the bindings instead
/// [MEM-OWNERSHIP-STATIC-BORROWS].
pub(super) fn note_bound_list(cg: &mut Codegen, stmt: &osprey_ast::Stmt) {
    let osprey_ast::Stmt::Let {
        mutable: false,
        value: Expr::List(items, Some(at)),
        ..
    } = stmt
    else {
        return;
    };
    let roots: Option<Vec<&str>> = items.iter().map(place_root).collect();
    let kept = |root: &&str| !cg.arc.movable.names_it(root);
    if cg.ownership.on && roots.is_some_and(|roots| roots.iter().any(kept)) {
        let _ = cg.ownership.lent_lists.insert((at.line, at.column));
    }
}

/// Whether the closure written at `position` lends its captures.
pub(crate) fn lends(cg: &Codegen, position: Option<Position>) -> bool {
    position.is_some_and(|at| cg.ownership.lent_closures.contains(&(at.line, at.column)))
}

/// Whether the list literal written at `position` only lends: it is passed
/// straight to a checked callee, so it dies with the statement and holds its
/// elements without owning them. Their owners all outlive it.
pub(crate) fn lends_list(cg: &Codegen, position: Option<Position>) -> bool {
    cg.ownership.on
        && position.is_some_and(|at| cg.ownership.lent_lists.contains(&(at.line, at.column)))
}

/// The next Result wraps an element read out of a collection: under ownership
/// it holds that element without owning it.
pub(crate) fn reading(cg: &mut Codegen) {
    cg.ownership.reading = cg.ownership.on;
}

/// Whether the Result being built wraps a read element, asked once.
pub(crate) fn take_reading(cg: &mut Codegen) -> bool {
    std::mem::take(&mut cg.ownership.reading)
}

/// Whether the ledger holds `v` as something that only lends.
pub(crate) fn is_lent(cg: &Codegen, v: &Value) -> bool {
    let held = super::aliases::root(cg, &v.operand);
    let mut entries = cg.arc.frames.iter().flatten();
    entries.any(|e| e.lent && e.operand == held)
}

/// Build the next closure cell without owning its captures, or as usual.
pub(crate) fn set_lending(cg: &mut Codegen, lending: bool) {
    cg.arc.lending = lending;
}

/// The just-owned cell `v` holds borrowed captures: it must die with the
/// statement that made it, so nothing may take it over.
pub(crate) fn mark_lent(cg: &mut Codegen, v: &Value) {
    let newest = cg.arc.frames.last_mut().and_then(|frame| frame.last_mut());
    if let Some(entry) = newest.filter(|entry| entry.operand == v.operand) {
        entry.lent = true;
        if let Some(name) = cg.arc.lending_name.take() {
            let _ = cg.arc.lent_names.insert(v.operand.clone(), name);
        }
    }
}

/// How the closure cell being built holds the captured value `capture`
/// [MEM-OWNERSHIP-STATIC-CLOSURES]: lent whole when it is an argument, else
/// each capture moves in at its last use and is lent otherwise.
pub(crate) fn hold_capture(cg: &mut Codegen, capture: &Value) -> Held {
    let slot = capture.llvm_ty();
    let managed = crate::meta::MetaField::of_slot_ty(&slot) == crate::meta::MetaField::PtrManaged;
    if cg.arc.lending {
        return Held::Lent;
    }
    if !managed || slot.contains('(') || !cg.arc.may_lend_captures {
        return Held::Stored;
    }
    let held = hold_field(cg, capture);
    cg.arc.lent_capture |= held.lends();
    held
}

/// The next closure cell is a closure literal's own: one that stays in the
/// scope that wrote it may lend a capture. A fiber's cell never does.
pub(crate) fn allow_lent_captures(cg: &mut Codegen) {
    cg.arc.may_lend_captures = true;
}

/// Whether the closure cell just built holds a lent capture.
pub(crate) fn take_lent_capture(cg: &mut Codegen) -> bool {
    cg.arc.may_lend_captures = false;
    std::mem::take(&mut cg.arc.lent_capture)
}

/// How a constructor holds one field [MEM-OWNERSHIP-STATIC-BORROWS]. Each way
/// is an action of the ledger model [MEM-OWNERSHIP-STATIC-MODEL]: `Moved` is
/// `move`, `Lent` is `borrow`. Proved there: a lent field pins its lender
/// (`lent_pins_owner`), is never freed by its holder (`lent_field_not_freed`)
/// and is freed once by its owner (`lent_then_released`, `lent_twice_released`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Held {
    /// Counted or moved by the store itself: every backend but ownership.
    Stored,
    /// The field took the one reference a fresh value or a last use gave.
    Moved,
    /// The field holds a value another owner keeps: the new value frees
    /// nothing through it and may not outlive that owner.
    Lent,
    /// The field took a fresh value that itself holds something lent: the new
    /// value owns it, and may not outlive what it borrows.
    MovedLent,
}

impl Held {
    /// Whether the value holding a field this way must die before a lender.
    pub(crate) fn lends(self) -> bool {
        matches!(self, Held::Lent | Held::MovedLent)
    }
}

/// Take the fresh value `operand` out of the statement's region when it only
/// lends, for the value being built to own in its place.
fn take_lent_temporary(cg: &mut Codegen, operand: &str) -> bool {
    let floor = super::conditional_floor(cg);
    let operand = super::aliases::root(cg, operand).to_owned();
    let taken = cg.arc.frames.last_mut().and_then(|frame| {
        let at = frame
            .iter()
            .rposition(|e| e.operand == operand && e.name.is_none() && e.lent)?;
        (at >= floor).then(|| frame.remove(at).slot)
    });
    if let Some(slot) = &taken {
        cg.emit(format!("store i8* null, i8** {slot}"));
        cg.arc.lending_name = lent_name(cg, &operand);
    }
    taken.is_some()
}

/// How a constructor holds the fresh value `pointer`, written in place as a
/// field: one that only lends moves in and makes the new value lend too.
pub(crate) fn hold_temporary(cg: &mut Codegen, pointer: &Value) -> Held {
    if cg.ownership.on && take_lent_temporary(cg, &pointer.operand) {
        Held::MovedLent
    } else {
        Held::Stored
    }
}

/// Decide how a constructor holds the managed pointer `pointer`. Under
/// ownership a value that can't move in is lent, never counted.
pub(crate) fn hold_field(cg: &mut Codegen, pointer: &Value) -> Held {
    let counted = pointer.operand.starts_with('%') && !cg.is_rodata(&pointer.operand);
    if !cg.ownership.on || !counted {
        return Held::Stored;
    }
    if super::consume_into_store(cg, &pointer.operand) {
        Held::Moved
    } else if take_lent_temporary(cg, &pointer.operand) {
        Held::MovedLent
    } else {
        cg.arc.lending_name = cg.name_of(&pointer.operand);
        Held::Lent
    }
}

/// The binding the lending value `operand` holds without owning, for a
/// refusal of that value to name.
pub(super) fn lent_name(cg: &Codegen, operand: &str) -> Option<String> {
    cg.arc.lent_names.get(operand).cloned()
}
