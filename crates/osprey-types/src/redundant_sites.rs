//! Locating and erasing written type annotations.
//!
//! Implements [TYPE-ANNOTATION-REDUNDANT]: the rule is decided by re-inferring
//! a program with one annotation replaced by a fresh variable, so the detector
//! needs a way to address each annotation and produce that erased copy. Both
//! needs are one deterministic pre-order walk over the same slots, so this
//! module exposes exactly one traversal and two thin drivers over it.

use std::collections::{HashMap, HashSet};

use osprey_ast::mutate::{children_mut, statement_children_mut};
use osprey_ast::{walk_program, AstVisitor, Expr, Position, Program, Stmt, TypeExpr};

use crate::check::annotation_name;
use crate::convert::type_expr_to_type;

/// What a written annotation is attached to, for the diagnostic's wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Slot {
    /// One ML signature, including every lowered curry fragment.
    Signature { owner: String },
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
    /// Slots derived from this one written annotation.
    pub(crate) members: Vec<usize>,
    /// Source identity shared by fragments of a written signature.
    pub(crate) source: Option<Position>,
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
                members: vec![found.len()],
                source: annotation.as_ref().and_then(|ty| ty.position),
                slot,
                written,
                position,
            });
        }
    });
    group_signatures(found)
}

/// Group only fragments with the same recorded source identity. Matching
/// types or matching lambda shapes cannot prove common provenance.
fn group_signatures(sites: Vec<Site>) -> Vec<Site> {
    let mut groups: Vec<Vec<Site>> = Vec::new();
    let mut sources = HashMap::new();
    for site in sites {
        let index = site
            .source
            .map(|p| (p.line, p.column))
            .map_or(groups.len(), |key| {
                *sources.entry(key).or_insert(groups.len())
            });
        match groups.get_mut(index) {
            Some(group) => group.push(site),
            None => groups.push(vec![site]),
        }
    }
    groups
        .iter()
        .filter_map(|group| signature_site(group))
        .collect()
}

fn signature_site(group: &[Site]) -> Option<Site> {
    let mut site = group.first()?.clone();
    if group.len() == 1 {
        return Some(site);
    }
    let (Slot::Param { owner, .. } | Slot::Return { owner }) = &site.slot else {
        return Some(site);
    };
    // Keep the outer function's return tail intact: a curried function and
    // a flat multi-parameter function have different types and call ABIs.
    let params: Vec<_> = group
        .iter()
        .take_while(|s| matches!(s.slot, Slot::Param { .. }))
        .map(|s| s.written.as_str())
        .collect();
    let result = group
        .iter()
        .find(|s| matches!(&s.slot, Slot::Return { owner: name } if name == owner))?;
    site.written = format!("({}) -> {}", params.join(", "), result.written);
    site.slot = Slot::Signature {
        owner: owner.clone(),
    };
    site.position = site.source;
    site.members = group
        .iter()
        .flat_map(|s| s.members.iter().copied())
        .collect();
    Some(site)
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

/// ML binders and effect rows share the type header. Type-slot erasure cannot
/// prove that deleting those other parts preserves the function's contract.
fn retained_header(statement: &Stmt) -> Option<Position> {
    let Stmt::Function {
        type_params,
        effects,
        return_type,
        position,
        ..
    } = statement
    else {
        return None;
    };
    let source = return_type.as_ref()?.position?;
    ((!type_params.is_empty() || !effects.is_empty())
        && source.line > 0
        && source.line < position.as_ref()?.line)
        .then_some(source)
}

fn retained_headers(program: &Program) -> HashSet<(u32, u32)> {
    #[derive(Default)]
    struct Headers(HashSet<(u32, u32)>);
    impl AstVisitor for Headers {
        fn statement(&mut self, statement: &Stmt) {
            if let Some(source) = retained_header(statement) {
                let _ = self.0.insert((source.line, source.column));
            }
        }
    }
    let mut headers = Headers::default();
    walk_program(program, &mut headers);
    headers.0
}

/// Visit written annotations, retaining module contracts and non-type headers.
fn walk(program: &mut Program, visit: Visit<'_>) {
    let retained = retained_headers(program);
    walk_statements(
        &mut program.statements,
        &mut |slot, position, annotation| {
            if !annotation.as_ref().is_some_and(|ty| {
                ty.is_from_contract()
                    || ty
                        .position
                        .is_some_and(|p| retained.contains(&(p.line, p.column)))
            }) {
                visit(slot, position, annotation);
            }
        },
    );
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
        } => visit(
            Slot::Binding {
                name: annotation_name(name),
            },
            *position,
            ty,
        ),
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
            parameter: annotation_name(&parameter.name),
            index,
        };
        visit(slot, position, &mut parameter.ty);
    }
    let slot = Slot::Return {
        owner: owner.to_string(),
    };
    visit(slot, position, return_type);
}
