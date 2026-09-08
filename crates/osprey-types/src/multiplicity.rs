//! The multiplicity axis's compile-time obligations.
//!
//! [`Multiplicity`] rides on the operation declaration, so every mention of an
//! operation in every row carries how many times its request may be answered.
//! This module turns that declaration into the four rejections it buys: the
//! affine rule on a `once` arm, the no-`resume` rule on an `abort` arm, and the
//! replay obligations a multi-shot handler region owes.
//!
//! The purely syntactic rules live here; the handle-site rules that need the
//! handled expression's effect row are driven from
//! [`effect_rows`](crate::effect_rows), which is the pass that already builds
//! it. Implements [MULTI-HANDLE], [MULTI-REPLAY].

use crate::TypeError;
use osprey_ast::{
    freevars::free_idents, resumes_on_one_path, walk_program, AstVisitor, Expr, HandlerArm,
    Multiplicity, OperationTable, Program, Stmt,
};
use std::collections::BTreeSet;

/// The declared-operation index plus the `mut` bindings a `many` arm may not
/// capture. The index itself lives in `osprey-ast` beside [`Multiplicity`], so
/// the checker and the per-target capability gate read one table rather than
/// two that can disagree about the same program.
#[derive(Debug, Default)]
pub(crate) struct OperationAxis {
    table: OperationTable,
    /// Every name bound with `mut` anywhere in the program. Read as a
    /// conservative over-approximation: a `many` arm closing over any of these
    /// names is rejected, which fails closed the way [MULTI-REPLAY-STATE]
    /// requires.
    mutable_bindings: BTreeSet<String>,
}

impl OperationAxis {
    /// Index every effect declaration and `mut` binding in `program`.
    pub(crate) fn collect(program: &Program) -> Self {
        let mut mutables = MutableBindings::default();
        walk_program(program, &mut mutables);
        Self {
            table: OperationTable::collect(program),
            mutable_bindings: mutables.0,
        }
    }

    /// How many times `effect.operation` may be answered ([MULTI-COMPAT]).
    pub(crate) fn multiplicity_of(&self, effect: &str, operation: &str) -> Multiplicity {
        self.table.multiplicity_of(effect, operation)
    }

    /// Whether re-performing `effect.operation` is acceptable ([MULTI-REPLAY]).
    pub(crate) fn is_replayable(&self, effect: &str, operation: &str) -> bool {
        self.table.is_replayable(effect, operation)
    }

    /// The `mut` bindings an arm body closes over, sorted. Implements
    /// [MULTI-REPLAY-STATE].
    pub(crate) fn captured_mutables(&self, body: &Expr) -> Vec<String> {
        let mut free = BTreeSet::new();
        free_idents(body, &mut free);
        free.intersection(&self.mutable_bindings).cloned().collect()
    }
}

/// Every name bound with `mut`, at any nesting depth.
#[derive(Default)]
struct MutableBindings(BTreeSet<String>);

impl AstVisitor for MutableBindings {
    fn statement(&mut self, statement: &Stmt) {
        if let Stmt::Let {
            name,
            mutable: true,
            ..
        } = statement
        {
            let _ = self.0.insert(name.clone());
        }
    }
}

/// Every obligation one handler arm owes its operation's declared multiplicity,
/// in the order that makes each diagnostic the informative one: what the arm or
/// region does wrong first, and only then what the compiler cannot yet
/// represent. Implements [MULTI-HANDLE], [MULTI-REPLAY].
///
/// `handled_row` is every operation the handled expression requires, as
/// `(effect, operation)` pairs — the row [MULTI-REPLAY-COARSE] reads instead of
/// per-`perform` control flow. `fibered` names an operation of `effect` the
/// handled expression performs inside a `spawn`.
pub(crate) fn arm_errors(
    axis: &OperationAxis,
    effect: &str,
    arm: &HandlerArm,
    handled_row: &BTreeSet<(String, String)>,
    fibered: Option<&str>,
) -> Vec<TypeError> {
    let operation = &arm.operation;
    let at = |message: String| vec![TypeError::new(message).with_pos(arm.position)];
    let resumes = resumes_on_one_path(&arm.body);
    match axis.multiplicity_of(effect, operation) {
        Multiplicity::Once if resumes > 1 => at(once_message(effect, operation)),
        Multiplicity::Once => Vec::new(),
        Multiplicity::Abort if resumes > 0 => at(abort_message(effect, operation)),
        Multiplicity::Abort => at(unimplemented_abort(effect, operation)),
        Multiplicity::Many => at(many_message(axis, effect, arm, handled_row, fibered)),
    }
}

/// Affine, not linear: zero resumptions stay legal, and two on DIFFERENT
/// branches of a `match` are one apiece on their own path. Implements
/// [MULTI-HANDLE-ONCE].
fn once_message(effect: &str, operation: &str) -> String {
    format!(
        "handler arm `{effect}.{operation}` may resume more than once; `{effect}.{operation}` is declared `once`"
    )
}

/// An `abort` arm's value answers the whole region and the `perform` never
/// returns, so a continuation it could resume does not exist. Implements
/// [MULTI-HANDLE-ABORT].
fn abort_message(effect: &str, operation: &str) -> String {
    format!(
        "handler arm `{effect}.{operation}` resumes; `{effect}.{operation}` is declared `abort`"
    )
}

/// The replay obligations of a multi-shot region in order, then the
/// representation it needs. The order is what keeps each rule's diagnostic the
/// informative one: what the region does wrong is reported before what the
/// compiler cannot yet build. Implements [MULTI-REPLAY-CHECK],
/// [MULTI-REPLAY-STATE], [MULTI-REPLAY-FIBER], [MULTI-HANDLE-MANY].
fn many_message(
    axis: &OperationAxis,
    effect: &str,
    arm: &HandlerArm,
    handled_row: &BTreeSet<(String, String)>,
    fibered: Option<&str>,
) -> String {
    non_replayable_entry(axis, effect, &arm.operation, handled_row)
        .or_else(|| captured_state(axis, effect, arm))
        .or_else(|| fiber_boundary(effect, &arm.operation, fibered))
        .unwrap_or_else(|| unimplemented_many(effect, &arm.operation))
}

/// The shared opening of the two rules that read the handled expression.
fn handled_opening(effect: &str, operation: &str) -> String {
    format!("handler for `{effect}.{operation}` may resume more than once, but the handled")
}

/// The first entry of the handled expression's row, other than `effect`'s own,
/// that is not replayable. Implements [MULTI-REPLAY-CHECK], [MULTI-REPLAY-COARSE].
fn non_replayable_entry(
    axis: &OperationAxis,
    effect: &str,
    operation: &str,
    handled_row: &BTreeSet<(String, String)>,
) -> Option<String> {
    let opening = handled_opening(effect, operation);
    handled_row
        .iter()
        .find(|(row_effect, row_op)| {
            row_effect != effect && !axis.is_replayable(row_effect, row_op)
        })
        .map(|(offender_effect, offender_op)| {
            format!(
                "{opening} expression requires non-replayable effect \
`{offender_effect}.{offender_op}`; the whole handled expression's row is read, so move the \
non-replayable work outside the region"
            )
        })
}

/// Handler-owned state is a single shared heap cell, so a second resumption
/// would observe the first's writes. Implements [MULTI-REPLAY-STATE].
fn captured_state(axis: &OperationAxis, effect: &str, arm: &HandlerArm) -> Option<String> {
    let operation = &arm.operation;
    axis.captured_mutables(&arm.body).first().map(|captured| {
        format!(
            "handler arm `{effect}.{operation}` captures mutable binding `{captured}`; a `many` arm \
cannot own state — combine resumptions through the value `resume` returns"
        )
    })
}

/// A resuming handler serializes one suspend-to-resume round trip per perform,
/// and a second resumption of a continuation spanning a spawned fiber has no
/// order to belong to. Implements [MULTI-REPLAY-FIBER].
fn fiber_boundary(effect: &str, operation: &str, fibered: Option<&str>) -> Option<String> {
    let opening = handled_opening(effect, operation);
    fibered.map(|fiber_operation| {
        format!(
            "{opening} expression performs `{effect}.{fiber_operation}` inside a fiber; a second \
resumption of a continuation spanning a spawned fiber has no serialization order"
        )
    })
}

/// A region that breaks none of the replay rules is well formed and the
/// compiler still cannot run it. Saying so is the whole obligation: accepting
/// it would answer a re-entrant request with a continuation that cannot be
/// re-entered, which is a silently wrong answer. Implements [MULTI-COST].
fn unimplemented_many(effect: &str, operation: &str) -> String {
    format!(
        "`{effect}.{operation}` is declared `many`, but a re-entrant continuation does not exist: \
native resume is one suspended stack, switched to and never switched back \
(docs/plans/0016-algebraic-effects-and-handlers.md). Declare `{effect}.{operation}` `once`"
    )
}

/// An `abort` arm reads its mode from the DECLARATION, so a `resume`-free arm
/// must abandon the region rather than substitute its value ([MULTI-HANDLE-ABORT-MODE]).
/// Abandoning needs the unwinding [MULTI-COST-ABORT] requires — `finally` arms
/// run and the discarded frames' owned operands released — which
/// docs/plans/0026-structured-concurrency.md has yet to build. Compiling the arm
/// under the substituting rule would make the `perform` RETURN, which is the
/// opposite of what the declaration says. Implements [MULTI-COST].
fn unimplemented_abort(effect: &str, operation: &str) -> String {
    format!(
        "`{effect}.{operation}` is declared `abort`, but abandoning a region does not yet unwind it: \
the discarded frames' owned operands are not released \
(docs/plans/0026-structured-concurrency.md). Remove `abort` and answer \
`{effect}.{operation}` with an arm that does not resume"
    )
}
