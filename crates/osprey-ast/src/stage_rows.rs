//! What a body requires, and at which stage.
//!
//! One walk answers both questions the staging axis asks of a program. At
//! [`Stage::Static`] it is the **dependency set** — the reactive reads a
//! function may make, including an explicit unknown remainder when a callback
//! has no published effect row ([STAGE-SIGNALS-EXACT]). At [`Stage::Dynamic`] it is
//! the **residual runtime row** — what a `kernel` boundary must find empty,
//! because device code cannot leave the device to reach a handler
//! ([STAGE-GPU-LEGAL]). They differ only in which declarations count, so they
//! share a walk rather than drifting as two.

use crate::mutate::children_mut;
use crate::stage::{effect_declarations, EffectDecl};
use crate::{effect_name, walk_program, AstNode, AstVisitor, Expr, Program, Stage, Stmt};
use std::collections::{BTreeMap, BTreeSet};

const UNKNOWN_DEPENDENCY: &str = "<unknown>";

/// The static-effect operations each named function requires, transitively
/// through the calls it makes, minus everything it discharges itself.
/// Implements [STAGE-SIGNALS-DIRTY].
///
/// This is the dependency set: a reactive read is a static operation, so the
/// row a function already carries names what it may read. The query runs on
/// the program **before** [`discharge`] erases those operations. An unknown
/// callback row is reported as `<unknown>` instead of an empty dependency set.
#[must_use]
pub fn dependencies(program: &Program) -> BTreeMap<String, Vec<String>> {
    let mut required = requirements(program, Stage::Static);
    attach_unknown(program, &mut required);
    required
}

fn attach_unknown(program: &Program, required: &mut BTreeMap<String, Vec<String>>) {
    for name in unknown_dependency_functions(program) {
        let operations = required.entry(name).or_default();
        operations.push(UNKNOWN_DEPENDENCY.to_owned());
        operations.sort();
        operations.dedup();
    }
}

/// A parameter invoked as a callable has an unresolved row. Propagate that
/// remainder through named callers instead of publishing a complete-looking
/// empty set that would miss reactive invalidation ([STAGE-SIGNALS-EXACT]).
fn unknown_dependency_functions(program: &Program) -> BTreeSet<String> {
    #[derive(Default)]
    struct Parameters(BTreeMap<String, BTreeSet<String>>);
    impl AstVisitor for Parameters {
        fn statement(&mut self, statement: &Stmt) {
            let callable = match statement {
                Stmt::Function {
                    name, parameters, ..
                }
                | Stmt::Let {
                    name,
                    value: Expr::Lambda { parameters, .. },
                    ..
                } => Some((name, parameters)),
                _ => None,
            };
            if let Some((name, parameters)) = callable {
                let _ = self.0.insert(
                    name.clone(),
                    parameters
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect(),
                );
            }
        }
    }
    let mut parameters = Parameters::default();
    walk_program(program, &mut parameters);
    let bodies = function_bodies(program);
    let mut unknown: BTreeSet<String> = bodies
        .iter()
        .filter(|(name, body)| {
            parameters
                .0
                .get(*name)
                .is_some_and(|params| invokes_parameter(body, params))
        })
        .map(|(name, _)| name.clone())
        .collect();
    loop {
        let previous = unknown.len();
        let inherited: Vec<_> = bodies
            .iter()
            .filter(|(_, body)| references_any(body, &unknown))
            .map(|(name, _)| name.clone())
            .collect();
        unknown.extend(inherited);
        if unknown.len() == previous {
            return unknown;
        }
    }
}

pub(crate) fn invokes_parameter(body: &Expr, parameters: &BTreeSet<String>) -> bool {
    let parameters = parameter_aliases(body, parameters);
    any_expression(body, |expression| {
        let Expr::Call { function, .. } = expression else {
            return false;
        };
        let mut names = BTreeSet::new();
        crate::freevars::free_idents(function, &mut names);
        !names.is_disjoint(&parameters)
    })
}

/// A value derived from an unresolved callable still has an unresolved row
/// when called through a local name. Widen conservatively for aliases and
/// wrappers whose body captures that callable.
pub(crate) fn parameter_aliases(body: &Expr, parameters: &BTreeSet<String>) -> BTreeSet<String> {
    let mut aliases = parameters.clone();
    loop {
        let previous = aliases.len();
        let mut pending = vec![AstNode::Expression(body)];
        while let Some(node) = pending.pop() {
            if let AstNode::Statement(Stmt::Let { name, value, .. }) = node {
                let mut names = BTreeSet::new();
                crate::freevars::free_idents(value, &mut names);
                if !names.is_disjoint(&aliases) {
                    let _ = aliases.insert(name.clone());
                }
            }
            node.for_each_child(|child| pending.push(child));
        }
        if aliases.len() == previous {
            return aliases;
        }
    }
}

fn references_any(body: &Expr, names: &BTreeSet<String>) -> bool {
    any_expression(
        body,
        |expression| matches!(expression, Expr::Identifier(name) if names.contains(name)),
    )
}

fn any_expression(body: &Expr, predicate: impl Fn(&Expr) -> bool) -> bool {
    let mut pending = vec![AstNode::Expression(body)];
    while let Some(node) = pending.pop() {
        if let AstNode::Expression(expression) = node {
            if predicate(expression) {
                return true;
            }
        }
        node.for_each_child(|child| pending.push(child));
    }
    false
}

/// Every function's transitive requirements at one stage, minus what it answers
/// itself. One walk serves both questions because they differ only in which
/// declarations count: [`Stage::Static`] is the dependency set
/// ([STAGE-SIGNALS-DIRTY]), [`Stage::Dynamic`] is the residual runtime row a
/// `kernel` boundary must find empty ([STAGE-GPU-LEGAL]).
pub(crate) fn requirements(program: &Program, stage: Stage) -> BTreeMap<String, Vec<String>> {
    let mut required = collect_requirements(program, stage, false);
    if stage == Stage::Dynamic {
        attach_unknown(program, &mut required);
    }
    required
}

/// Runtime dispatch remains runtime work even when a helper handles it locally.
/// Static arm validation must inspect it separately from unhandled obligations.
pub(crate) fn dispatch_requirements(program: &Program) -> BTreeMap<String, Vec<String>> {
    collect_requirements(program, Stage::Dynamic, true)
}

fn collect_requirements(
    program: &Program,
    stage: Stage,
    dispatch: bool,
) -> BTreeMap<String, Vec<String>> {
    let effects = effect_declarations(program);
    let facts: BTreeMap<String, BodyFacts> = function_bodies(program)
        .iter()
        .map(|(name, body)| (name.clone(), body_facts(body, &effects, stage, dispatch)))
        .collect();
    let mut required: BTreeMap<String, Vec<String>> = facts
        .iter()
        .map(|(name, fact)| (name.clone(), fact.direct.clone()))
        .collect();
    for _ in 0..=facts.len() {
        let propagated = propagate(&facts, &required);
        if propagated == required {
            break;
        }
        required = propagated;
    }
    required
}

/// What one function body contributes to its own dependency set: the static
/// operations it performs outside any region that answers them, and the names
/// it references together with the operations already answered at that point.
struct BodyFacts {
    direct: Vec<String>,
    references: Vec<(String, Vec<String>)>,
}

/// What ONE body still requires at `stage`, given the program-wide fixed point
/// `required`. This is the query a region boundary asks — a `kernel` asks it of
/// the runtime stage ([STAGE-GPU-LEGAL]) — where the fixed point asks it of
/// every named function.
pub(crate) fn body_requirements(
    effects: &BTreeMap<String, EffectDecl>,
    body: &Expr,
    stage: Stage,
    required: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    resolve(&body_facts(body, effects, stage, false), required)
}

pub(crate) fn body_dispatches(
    effects: &BTreeMap<String, EffectDecl>,
    body: &Expr,
    required: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    resolve(&body_facts(body, effects, Stage::Dynamic, true), required)
}

/// Every function's residual RUNTIME row: what a `kernel` boundary must find
/// empty. Implements [STAGE-GPU-LEGAL].
pub(crate) fn runtime_requirements(program: &Program) -> BTreeMap<String, Vec<String>> {
    requirements(program, Stage::Dynamic)
}

/// One fixed-point step: each function gains every requirement of what it
/// references, except the operations a region already answered around it.
fn propagate(
    facts: &BTreeMap<String, BodyFacts>,
    required: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<String>> {
    facts
        .iter()
        .map(|(name, fact)| (name.clone(), resolve(fact, required)))
        .collect()
}

/// One body's requirements: what it performs itself plus what the names it
/// references still need, minus whatever a region around that reference already
/// answers. The kernel boundary asks this of a single body, so it lives apart
/// from the fixed point that asks it of every function.
fn resolve(fact: &BodyFacts, required: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut operations = fact.direct.clone();
    for (callee, handled) in &fact.references {
        let inherited = required.get(callee).into_iter().flatten();
        operations.extend(
            inherited
                .filter(|operation| !handled.contains(*operation))
                .cloned(),
        );
    }
    sorted(operations)
}

/// The `Effect.op` name every requirement, dependency and dispatch is keyed by.
pub(crate) fn operation_name(effect: &str, operation: &str) -> String {
    format!("{effect}.{operation}")
}

fn sorted(mut operations: Vec<String>) -> Vec<String> {
    operations.sort();
    operations.dedup();
    operations
}

/// Walk one body, tracking which operations an enclosing region already
/// answers so a self-handled operation never counts as a dependency.
fn body_facts(
    body: &Expr,
    effects: &BTreeMap<String, EffectDecl>,
    stage: Stage,
    dispatch: bool,
) -> BodyFacts {
    let mut facts = BodyFacts {
        direct: Vec::new(),
        references: Vec::new(),
    };
    scan_body(
        &mut body.clone(),
        effects,
        stage,
        dispatch,
        &mut Vec::new(),
        &mut facts,
    );
    facts.direct = sorted(std::mem::take(&mut facts.direct));
    facts
}

fn scan_body(
    expression: &mut Expr,
    effects: &BTreeMap<String, EffectDecl>,
    stage: Stage,
    dispatch: bool,
    handled: &mut Vec<String>,
    facts: &mut BodyFacts,
) {
    match expression {
        // Discharge obligations in either stage, but retain runtime requests
        // when querying dispatch rather than unhandled requirements. A handler
        // answers the operations its arms supply, not its whole effect: an
        // uncovered operation still searches outward ([STAGE-LOWER-ORDER]).
        Expr::Handler {
            effect,
            arms,
            body,
            return_clause,
            stage: selected,
            ..
        } => {
            let effect = effect.clone();
            for arm in arms.iter_mut() {
                scan_body(&mut arm.body, effects, stage, dispatch, handled, facts);
            }
            let depth = handled.len();
            if !dispatch || selected.is_compile_time() {
                handled.extend(
                    arms.iter()
                        .map(|arm| operation_name(&effect, &arm.operation)),
                );
            }
            scan_body(body, effects, stage, dispatch, handled, facts);
            handled.truncate(depth);
            if let Some(clause) = return_clause {
                scan_body(clause, effects, stage, dispatch, handled, facts);
            }
        }
        Expr::Perform {
            effect, operation, ..
        } => {
            let requested = operation_name(effect, operation);
            if declared_at(effects, effect, stage) && !handled.contains(&requested) {
                facts.direct.push(requested);
            }
            children_mut(expression, &mut |child| {
                scan_body(child, effects, stage, dispatch, handled, facts);
            });
        }
        Expr::Identifier(name) => facts.references.push((name.clone(), handled.clone())),
        _ => children_mut(expression, &mut |child| {
            scan_body(child, effects, stage, dispatch, handled, facts);
        }),
    }
}

/// Whether `effect` is DECLARED at `stage`. An undeclared effect belongs to
/// neither: the checker that owns "unknown effect" reports it once, rather than
/// this walk reporting it again under another name.
fn declared_at(effects: &BTreeMap<String, EffectDecl>, effect: &str, stage: Stage) -> bool {
    effects
        .get(effect_name::base(effect))
        .is_some_and(|declared| declared.stage == stage)
}

/// Every named function in the program, at any nesting depth.
fn function_bodies(program: &Program) -> BTreeMap<String, Expr> {
    #[derive(Default)]
    struct Collector(BTreeMap<String, Expr>);
    impl AstVisitor for Collector {
        fn statement(&mut self, statement: &Stmt) {
            match statement {
                Stmt::Function { name, body, .. } => {
                    let _ = self.0.insert(name.clone(), body.clone());
                }
                Stmt::Let {
                    name,
                    mutable: false,
                    value,
                    ..
                } => match value {
                    Expr::Lambda { body, .. } => {
                        let _ = self.0.insert(name.clone(), (**body).clone());
                    }
                    Expr::Identifier(_) => {
                        let _ = self.0.insert(name.clone(), value.clone());
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    let mut collector = Collector::default();
    walk_program(program, &mut collector);
    collector.0
}
