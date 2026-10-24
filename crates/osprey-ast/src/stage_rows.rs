//! Structural requirements used during static lowering and kernel validation.
//!
//! Reactive dependency consumers use `osprey_types::signal_dependencies`,
//! which validates source and preserves resolved callable and generic identities.

use crate::mutate::children_mut;
use crate::stage::{effect_declarations, EffectDecl};
use crate::{effect_name, walk_program, AstNode, AstVisitor, Expr, Program, Stage, Stmt};
use std::collections::{BTreeMap, BTreeSet};

mod callables;

pub(crate) use callables::Callables;
use callables::UNKNOWN;

// Structural lowering must also fail closed on unresolved callback dispatch.
// Reactive dependency reporting instead uses the resolved checker proof.
fn attach_unknown(program: &Program, required: &mut BTreeMap<String, Vec<String>>) {
    for name in Callables::of(program).unknown() {
        required
            .entry(name.clone())
            .or_default()
            .push(UNKNOWN.to_owned());
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

/// Whether `body` performs an operation of an effect `selected` accepts.
pub(crate) fn any_perform(body: &Expr, selected: impl Fn(&str) -> bool) -> bool {
    any_expression(
        body,
        |expression| matches!(expression, Expr::Perform { effect, .. } if selected(effect)),
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
    requirements_with_bindings(program, stage, &BTreeMap::new())
}

/// Proven callable factories participate in the same transitive fixed point
/// as functions and lexical closures, including wrappers passing them onward.
pub(crate) fn requirements_with_bindings(
    program: &Program,
    stage: Stage,
    bindings: &BTreeMap<String, Expr>,
) -> BTreeMap<String, Vec<String>> {
    let mut required = collect_requirements(program, stage, false, bindings);
    if stage == Stage::Dynamic {
        attach_unknown(program, &mut required);
    }
    required
}

/// Runtime dispatch remains runtime work even when a helper handles it locally.
/// Static arm validation must inspect it separately from unhandled obligations.
pub(crate) fn dispatch_requirements(program: &Program) -> BTreeMap<String, Vec<String>> {
    collect_requirements(program, Stage::Dynamic, true, &BTreeMap::new())
}

fn collect_requirements(
    program: &Program,
    stage: Stage,
    dispatch: bool,
    bindings: &BTreeMap<String, Expr>,
) -> BTreeMap<String, Vec<String>> {
    let (effects, callables) = (effect_declarations(program), Callables::of(program));
    let scan = Scan {
        effects: &effects,
        callables: &callables,
        stage,
        dispatch,
    };
    let mut bodies = function_bodies(program);
    bodies.extend(
        bindings
            .iter()
            .map(|(name, body)| (name.clone(), body.clone())),
    );
    let facts: BTreeMap<String, BodyFacts> = bodies
        .iter()
        .map(|(name, body)| (name.clone(), body_facts(body, &scan)))
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
    /// Each referenced name, the operations answered there, and whether it is
    /// the callee of a call that settles its unknown ([`Callables::resolves`]).
    references: Vec<(String, Vec<String>, bool)>,
}

/// What ONE body still requires at `stage`, given the program-wide fixed point
/// `required`. This is the query a region boundary asks — a `kernel` asks it of
/// the runtime stage ([STAGE-GPU-LEGAL]) — where the fixed point asks it of
/// every named function.
pub(crate) fn body_requirements(
    effects: &BTreeMap<String, EffectDecl>,
    callables: &Callables,
    body: &Expr,
    stage: Stage,
    required: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let scan = Scan {
        effects,
        callables,
        stage,
        dispatch: false,
    };
    resolve(&body_facts(body, &scan), required)
}

pub(crate) fn body_dispatches(
    effects: &BTreeMap<String, EffectDecl>,
    callables: &Callables,
    body: &Expr,
    required: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let scan = Scan {
        effects,
        callables,
        stage: Stage::Dynamic,
        dispatch: true,
    };
    resolve(&body_facts(body, &scan), required)
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
    for (callee, handled, settled) in &fact.references {
        let inherited = required.get(callee).into_iter().flatten();
        operations.extend(
            inherited
                .filter(|operation| !answered(handled, operation))
                .filter(|operation| !(*settled && operation.as_str() == UNKNOWN))
                .cloned(),
        );
    }
    sorted(operations)
}

/// Whether an enclosing region already answers `operation`, including a
/// request that leaves its instantiation to inference.
fn answered(handled: &[String], operation: &str) -> bool {
    handled
        .iter()
        .any(|supplied| effect_name::operation_covers(operation, supplied))
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

/// One walk's fixed inputs: the declared effects, what call sites settle, the
/// stage whose requests count, and whether runtime requests a handler answers
/// still count as dispatches.
struct Scan<'a> {
    effects: &'a BTreeMap<String, EffectDecl>,
    callables: &'a Callables,
    stage: Stage,
    dispatch: bool,
}

/// Walk one body, tracking which operations an enclosing region already
/// answers so a self-handled operation never counts as a dependency.
fn body_facts(body: &Expr, scan: &Scan<'_>) -> BodyFacts {
    let mut facts = BodyFacts {
        direct: Vec::new(),
        references: Vec::new(),
    };
    scan_body(&mut body.clone(), scan, &mut Vec::new(), &mut facts);
    facts.direct = sorted(std::mem::take(&mut facts.direct));
    facts
}

fn scan_body(
    expression: &mut Expr,
    scan: &Scan<'_>,
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
            finally,
            stage: selected,
            ..
        } => {
            let effect = effect.clone();
            for arm in arms.iter_mut() {
                scan_body(&mut arm.body, scan, handled, facts);
            }
            let depth = handled.len();
            if !scan.dispatch || selected.is_compile_time() {
                handled.extend(
                    arms.iter()
                        .map(|arm| operation_name(&effect, &arm.operation)),
                );
            }
            scan_body(body, scan, handled, facts);
            handled.truncate(depth);
            for clause in return_clause.iter_mut().chain(finally.iter_mut()) {
                scan_body(clause, scan, handled, facts);
            }
        }
        Expr::Perform {
            effect, operation, ..
        } => {
            let requested = operation_name(effect, operation);
            if declared_at(scan.effects, effect, scan.stage) && !answered(handled, &requested) {
                facts.direct.push(requested);
            }
            children_mut(expression, &mut |child| {
                scan_body(child, scan, handled, facts);
            });
        }
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } if scan
            .callables
            .resolves(function, arguments, named_arguments) =>
        {
            if let Expr::Identifier(callee) = &**function {
                facts
                    .references
                    .push((callee.clone(), handled.clone(), true));
            }
            for argument in arguments.iter_mut() {
                scan_body(argument, scan, handled, facts);
            }
        }
        Expr::Identifier(name) => facts
            .references
            .push((name.clone(), handled.clone(), false)),
        _ => children_mut(expression, &mut |child| {
            scan_body(child, scan, handled, facts);
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
