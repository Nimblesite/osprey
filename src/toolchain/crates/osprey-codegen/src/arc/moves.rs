//! Moving a named owner at its last use [GC-ARC-PERCEUS] (Perceus TR §2.2,
//! "ownership transfer at the last use"; docs/specs/compiler/0018-MemoryManagement.md).
//!
//! A binding read exactly once by a statement, on a path that runs exactly
//! once, and never read after it, gives up its reference to the record, list or
//! binding that takes the value there. Nothing is counted and nothing is
//! released: the reference the binding held becomes the taker's.
use super::{Codegen, Entry};
use osprey_ast::{Expr, Stmt};
use std::collections::BTreeSet;

/// The bindings the statement being lowered may move, and where it started.
#[derive(Default)]
pub(crate) struct Movable {
    names: BTreeSet<String>,
    scope: Option<usize>,
    depth: usize,
    conditionals: usize,
}

/// Operators that always evaluate both operands, each once.
const STRICT: [&str; 11] = ["+", "-", "*", "/", "%", "==", "!=", "<", "<=", ">", ">="];

/// The expression an interpolated part embeds, if it embeds one.
fn embedded(part: &osprey_ast::InterpolatedPart) -> Option<&Expr> {
    match part {
        osprey_ast::InterpolatedPart::Expr(e) => Some(e),
        osprey_ast::InterpolatedPart::Text(_) => None,
    }
}

/// How often a statement reads each name.
#[derive(Default)]
struct Reads {
    once: BTreeSet<String>,
    more: BTreeSet<String>,
}

impl Reads {
    fn read(&mut self, name: &str) {
        if !self.once.insert(name.to_owned()) {
            let _ = self.more.insert(name.to_owned());
        }
    }

    /// Each name the lambda `e` closes over is read once, by its capture.
    fn captures(&mut self, e: &Expr) {
        let mut free = BTreeSet::new();
        osprey_ast::freevars::free_idents(e, &mut free);
        for name in &free {
            self.read(name);
        }
    }

    /// Every name `e` mentions is read some unknown number of times.
    fn unknown(&mut self, e: &Expr) {
        let mut free = BTreeSet::new();
        osprey_ast::freevars::free_idents(e, &mut free);
        self.more.extend(free);
    }

    /// Count reads through the forms whose operands each run exactly once.
    /// Anything else may branch, loop or defer, so what it mentions isn't
    /// counted as one read.
    fn visit(&mut self, e: &Expr) {
        match e {
            Expr::Integer(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) => {}
            Expr::Identifier(name) => self.read(name),
            // A closure cell is built once and takes each capture once.
            Expr::Lambda { .. } => self.captures(e),
            Expr::List(items, _) => self.visit_all(items),
            Expr::InterpolatedStr(parts) => self.visit_all(parts.iter().filter_map(embedded)),
            Expr::FieldAccess { target, .. } => self.visit(target),
            Expr::Unary { operand, .. } => self.visit(operand),
            Expr::Binary {
                op, left, right, ..
            } if STRICT.contains(&op.as_str()) => self.visit_all([&**left, &**right]),
            _ => self.visit_application(e),
        }
    }

    /// A constructor or a call: every field and argument runs once.
    fn visit_application(&mut self, e: &Expr) {
        match e {
            Expr::TypeConstructor { name, fields, .. } => {
                let _ = self.more.insert(name.clone());
                self.visit_all(fields.iter().map(|field| &field.value));
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                self.visit(function);
                self.visit_all(arguments);
                self.visit_all(named_arguments.iter().map(|argument| &argument.value));
            }
            _ => self.unknown(e),
        }
    }

    fn visit_all<'a>(&mut self, operands: impl IntoIterator<Item = &'a Expr>) {
        for operand in operands {
            self.visit(operand);
        }
    }

    fn sole(self) -> BTreeSet<String> {
        self.once.difference(&self.more).cloned().collect()
    }
}

/// The value expression a statement evaluates, if it is one that can move.
fn statement_value(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Expr { value, .. } => Some(value),
        _ => None,
    }
}

/// `value` is about to be lowered with `live` still read after it in its block:
/// note the bindings it may move. A binding moves only from the scope that made
/// it ([`take`]), so the rest of that block is all that can read it again.
pub(crate) fn begin(cg: &mut Codegen, value: &Expr, live: &BTreeSet<String>) {
    let mut reads = Reads::default();
    reads.visit(value);
    cg.arc.movable = Movable {
        names: reads.sole().difference(live).cloned().collect(),
        scope: cg.scope_id(),
        depth: cg.arc.frames.len(),
        conditionals: cg.arc.cond_floors.len(),
    };
}

/// [`begin`] for a block statement followed by `rest` and the block's `tail`.
pub(crate) fn begin_statement<S: std::borrow::Borrow<Stmt>>(
    cg: &mut Codegen,
    stmt: &Stmt,
    rest: &[S],
    tail: Option<&Expr>,
) {
    if let Some(value) = statement_value(stmt) {
        let live = super::last_use::live_after(cg, rest, tail);
        begin(cg, value, &live);
    }
}

/// The statement is lowered: nothing may move until the next one begins.
pub(crate) fn end(cg: &mut Codegen) {
    cg.arc.movable = Movable::default();
}

/// Take the ledger entry of the movable binding whose value is `operand`. The
/// lowering must still be where the statement started: in its scope, at most
/// its own statement frame deep, and inside no conditional opened since.
pub(super) fn take(cg: &mut Codegen, operand: &str) -> Option<Entry> {
    let movable = &cg.arc.movable;
    let here = movable.scope.is_some()
        && cg.scope_id() == movable.scope
        && cg.arc.frames.len() <= movable.depth + 1
        && cg.arc.cond_floors.len() == movable.conditionals;
    if !here {
        return None;
    }
    let scope = movable.scope;
    // A binding's owner is in the innermost scope frame ([`super::push_parent`]).
    let innermost = cg
        .arc
        .kinds
        .iter()
        .rposition(|kind| *kind == super::FrameKind::Scope)?;
    let frame = cg.arc.frames.get_mut(innermost)?;
    let at = frame.iter().rposition(|e| {
        e.operand == operand
            && e.binding_scope == scope
            && e.name
                .as_ref()
                .is_some_and(|name| movable.names.contains(name))
    })?;
    let entry = frame.remove(at);
    if let Some(name) = &entry.name {
        let _ = cg.arc.movable.names.remove(name);
    }
    Some(entry)
}
