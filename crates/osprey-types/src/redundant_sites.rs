//! Locating and erasing written type annotations.
//!
//! Implements [TYPE-ANNOTATION-REDUNDANT]: the rule is decided by re-inferring
//! a program with one annotation replaced by a fresh variable, so the detector
//! needs a way to address each annotation and produce that erased copy. Both
//! needs are one deterministic pre-order walk over the same slots, so this
//! module exposes exactly one traversal and two thin drivers over it.

use std::collections::HashMap;

use osprey_ast::mutate::{children_mut, statement_children_mut};
use osprey_ast::{Expr, Position, Program, Stmt, TypeExpr};

use crate::convert::type_expr_to_type;

/// What a written annotation is attached to, for the diagnostic's wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Slot {
    /// A named function or lambda parameter.
    Param {
        /// The enclosing function's name (`<lambda>` for an anonymous one).
        owner: String,
        /// The annotated parameter's name.
        parameter: String,
        /// Its position in the parameter list, which is how a signature — that
        /// names types and not parameters — addresses it.
        index: usize,
    },
    /// A function's or lambda's declared return type.
    Return {
        /// The enclosing function's name (`<lambda>` for an anonymous one).
        owner: String,
    },
    /// A `let` binding's declared type.
    Binding {
        /// The bound name.
        name: String,
    },
}

/// The name reported for an annotation written on an anonymous function.
const LAMBDA_OWNER: &str = "<lambda>";

/// One written annotation, addressed by its index in the pre-order walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site {
    /// What the annotation is attached to.
    pub(crate) slot: Slot,
    /// The written type, rendered. Redundancy means inference derives exactly
    /// this, so it is also the type the diagnostic reports as derived.
    pub(crate) written: String,
    /// Where the annotated declaration was written, when recorded.
    pub(crate) position: Option<Position>,
}

/// The callback every traversal function here threads through the tree: it
/// receives each annotation slot and the annotation itself, by unique
/// reference so a driver can clear it in place.
type Visit<'a> = &'a mut dyn FnMut(Slot, Option<Position>, &mut Option<TypeExpr>);

/// Every written annotation the rule judges, in walk order.
///
/// The index of a [`Site`] in this list is its address: [`erase`] clears the
/// annotation the same walk reaches at the same index.
pub(crate) fn sites(program: &Program) -> Vec<Site> {
    let mut found = Vec::new();
    let mut scratch = program.clone();
    walk(&mut scratch, &mut |slot, position, annotation| {
        if let Some(written) = annotation.as_ref().map(render) {
            found.push(Site {
                slot,
                written,
                position,
            });
        }
    });
    found
}

/// A copy of `program` with the annotations selected by `keep` removed.
///
/// `keep` is called with each site's walk index and returns `true` to leave the
/// annotation in place, so erasing one site and erasing every site are the same
/// traversal under two predicates.
pub(crate) fn erase(program: &Program, keep: &mut impl FnMut(usize) -> bool) -> Program {
    let mut erased = program.clone();
    let mut index = 0usize;
    walk(&mut erased, &mut |_, _, annotation| {
        if annotation.is_some() {
            if !keep(index) {
                *annotation = None;
            }
            index += 1;
        }
    });
    erased
}

/// Render a written annotation the way inference renders the type it derives,
/// so a diagnostic's before and after are directly comparable.
fn render(annotation: &TypeExpr) -> String {
    type_expr_to_type(annotation, &HashMap::new()).to_string()
}

/// Visit written annotations, retaining the types supplied by module contracts.
fn walk(program: &mut Program, visit: Visit<'_>) {
    walk_statements(&mut program.statements, &mut |slot, position, annotation| {
        if !annotation.as_ref().is_some_and(TypeExpr::is_from_contract) {
            visit(slot, position, annotation);
        }
    });
}

/// Walk a statement sequence — a program body, a namespace, or a block.
fn walk_statements(statements: &mut [Stmt], visit: Visit<'_>) {
    for statement in statements {
        walk_statement(statement, &mut *visit);
    }
}

/// Walk one statement's own annotation slots, then the expressions inside it.
///
/// A container statement recurses here and returns, because
/// [`statement_children_mut`] would otherwise reach the same nested statements
/// a second time and shift every later site's index.
fn walk_statement(statement: &mut Stmt, visit: Visit<'_>) {
    match statement {
        Stmt::Namespace { body, .. } => return walk_statements(body, &mut *visit),
        Stmt::Module { body, .. } => {
            for item in &mut *body {
                walk_statement(&mut item.declaration, &mut *visit);
            }
            return;
        }
        Stmt::Function {
            name,
            parameters,
            return_type,
            position,
            ..
        } => walk_signature(name, parameters, return_type, *position, &mut *visit),
        Stmt::Let {
            name, ty, position, ..
        } => visit(Slot::Binding { name: name.clone() }, *position, ty),
        _ => {}
    }
    statement_children_mut(statement, &mut |expression| {
        walk_expr(expression, &mut *visit);
    });
}

/// Walk the annotation slots reachable through an expression.
///
/// A block owns statements, which [`children_mut`] reaches only as their child
/// expressions, so a block recurses here instead of through that helper.
fn walk_expr(expression: &mut Expr, visit: Visit<'_>) {
    if let Expr::Lambda {
        parameters,
        return_type,
        position,
        ..
    } = expression
    {
        walk_signature(
            LAMBDA_OWNER,
            parameters,
            return_type,
            *position,
            &mut *visit,
        );
    }
    if let Expr::Block { statements, value } = expression {
        walk_statements(statements, &mut *visit);
        if let Some(value) = value {
            walk_expr(value, &mut *visit);
        }
        return;
    }
    children_mut(expression, &mut |child| walk_expr(child, &mut *visit));
}

/// Hand a callable's parameter and return annotations to `visit`, in order.
fn walk_signature(
    owner: &str,
    parameters: &mut [osprey_ast::Parameter],
    return_type: &mut Option<TypeExpr>,
    position: Option<Position>,
    visit: Visit<'_>,
) {
    for (index, parameter) in parameters.iter_mut().enumerate() {
        let slot = Slot::Param {
            owner: owner.to_string(),
            parameter: parameter.name.clone(),
            index,
        };
        visit(slot, position, &mut parameter.ty);
    }
    let slot = Slot::Return {
        owner: owner.to_string(),
    };
    visit(slot, position, return_type);
}
