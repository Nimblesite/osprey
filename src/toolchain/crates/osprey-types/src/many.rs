//! Multi-shot (`many`) continuations as ordinary closures.
//!
//! A `many` operation's handler may answer one request any number of times,
//! and every answer runs its own branch of the captured remainder. This pass
//! gives that remainder a representation every backend already runs: the code
//! after a request becomes a closure, a request becomes a value carrying that
//! closure, and a handler becomes a function over those values. Resuming is a
//! call, resuming twice is two calls, and abandoning is dropping the value.
//!
//! Nothing here depends on a machine stack, a thread, an architecture or an
//! operating system, so native, wasm32 and every other target give the same
//! answers. Implements [MULTI-HANDLE-MANY], [MULTI-HANDLE-MANY-LEXICAL],
//! [MULTI-COST], [MULTI-WASM] and the invariant
//! [EFFECTS-INV-MULTISHOT-PORTABLE].
//!
//! The pass runs on a program the source proof already accepted, so it adds no
//! effect rule of its own: it only rejects the few shapes a closure cannot
//! express, naming each one.

mod applied;
mod build;
mod escape;
mod facts;
mod helpers;
mod regions;
mod rewrite;
mod sequence;
mod table;

use crate::TypeError;
use osprey_ast::Program;

/// Whether `program` declares a `many` operation, which is the only thing
/// that makes this pass touch a program at all ([MULTI-COMPAT]).
pub(crate) fn applies(program: &Program) -> bool {
    table::declares_many(&program.statements)
}

/// Rewrite every continuation-capturing region of `program` into closures.
///
/// # Errors
/// Names each construct a continuation cannot cross.
/// The effects whose handlers run as closures rather than on a suspended stack,
/// so a target needs no continuation support of its own for them
/// ([MULTI-WASM]).
#[must_use]
pub fn captured_effects(program: &Program) -> std::collections::BTreeSet<String> {
    if !applies(program) {
        return std::collections::BTreeSet::new();
    }
    let table = table::Table::collect(program);
    escape::solve(&table, program).captured
}

pub(crate) fn lower(program: &Program) -> Result<Program, Vec<TypeError>> {
    let table = table::Table::collect(program);
    let facts = escape::solve(&table, program);
    let generic = helpers::generic_errors(&table, &facts);
    if !generic.is_empty() {
        return Err(generic);
    }
    let mut lowered = rewrite::program(&table, facts.clone(), program)?;
    helpers::attach(&mut lowered, &table, &facts);
    Ok(lowered)
}
