//! What a body requires, and at which stage.
//!
//! One walk answers both questions the staging axis asks of a program. At
//! [`Stage::Static`] it is the **dependency set** — the reactive reads a
//! function makes, which is exact because a static operation cannot be reached
//! except by performing it ([STAGE-SIGNALS-DIRTY]). At [`Stage::Dynamic`] it is
//! the **residual runtime row** — what a `kernel` boundary must find empty,
//! because device code cannot leave the device to reach a handler
//! ([STAGE-GPU-LEGAL]). They differ only in which declarations count, so they
//! share a walk rather than drifting as two.

use crate::mutate::children_mut;
use crate::stage::{effect_declarations, EffectDecl};
use crate::{effect_name, walk_program, AstVisitor, Expr, Program, Stage, Stmt};
use std::collections::BTreeMap;

/// The static-effect operations each named function requires, transitively
/// through the calls it makes, minus everything it discharges itself.
/// Implements [STAGE-SIGNALS-DIRTY].
///
/// This is the dependency set: a reactive read is a static operation, so the
/// row a function already carries names exactly the data it touches. The query
/// runs on the program **before** [`discharge`] erases those operations —
/// erasure is what makes the read free, and this is what makes it exact.
#[must_use]
pub fn dependencies(program: &Program) -> BTreeMap<String, Vec<String>> {
    requirements(program, Stage::Static)
}

/// Every function's transitive requirements at one stage, minus what it answers
/// itself. One walk serves both questions because they differ only in which
/// declarations count: [`Stage::Static`] is the dependency set
/// ([STAGE-SIGNALS-DIRTY]), [`Stage::Dynamic`] is the residual runtime row a
/// `kernel` boundary must find empty ([STAGE-GPU-LEGAL]).
pub(crate) fn requirements(program: &Program, stage: Stage) -> BTreeMap<String, Vec<String>> {
    let effects = effect_declarations(program);
    let facts: BTreeMap<String, BodyFacts> = function_bodies(program)
        .iter()
        .map(|(name, body)| (name.clone(), body_facts(body, &effects, stage)))
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
/// it references together with the effects already answered at that point.
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
    resolve(&body_facts(body, effects, stage), required)
}

/// Every function's residual RUNTIME row: what a `kernel` boundary must find
/// empty. Implements [STAGE-GPU-LEGAL].
pub(crate) fn runtime_requirements(program: &Program) -> BTreeMap<String, Vec<String>> {
    requirements(program, Stage::Dynamic)
}

/// One fixed-point step: each function gains every requirement of what it
/// references, except the effects a region already answered around it.
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
                .filter(|operation| !handled.iter().any(|e| owns(e, operation)))
                .cloned(),
        );
    }
    sorted(operations)
}

/// Whether `effect` declares `operation` (an `Effect.op` name).
fn owns(effect: &str, operation: &str) -> bool {
    operation.split('.').next() == Some(effect)
}

fn sorted(mut operations: Vec<String>) -> Vec<String> {
    operations.sort();
    operations.dedup();
    operations
}

/// Walk one body, tracking which static effects an enclosing region already
/// answers so a self-handled effect never counts as a dependency.
fn body_facts(body: &Expr, effects: &BTreeMap<String, EffectDecl>, stage: Stage) -> BodyFacts {
    let mut facts = BodyFacts {
        direct: Vec::new(),
        references: Vec::new(),
    };
    scan_body(
        &mut body.clone(),
        effects,
        stage,
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
    handled: &mut Vec<String>,
    facts: &mut BodyFacts,
) {
    match expression {
        // A region answers its own effect for its body but not for its arms —
        // whichever stage it is, since a handler's stage must match its effect's.
        Expr::Handler {
            effect, arms, body, ..
        } => {
            let effect = effect.clone();
            for arm in arms {
                scan_body(&mut arm.body, effects, stage, handled, facts);
            }
            handled.push(effect);
            scan_body(body, effects, stage, handled, facts);
            let _ = handled.pop();
        }
        Expr::Perform {
            effect, operation, ..
        } => {
            let (effect, operation) = (effect.clone(), operation.clone());
            if declared_at(effects, &effect, stage) && !handled.contains(&effect) {
                facts.direct.push(format!("{effect}.{operation}"));
            }
            children_mut(expression, &mut |child| {
                scan_body(child, effects, stage, handled, facts);
            });
        }
        Expr::Identifier(name) => facts.references.push((name.clone(), handled.clone())),
        _ => children_mut(expression, &mut |child| {
            scan_body(child, effects, stage, handled, facts);
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
            if let Stmt::Function { name, body, .. } = statement {
                let _ = self.0.insert(name.clone(), body.clone());
            }
        }
    }
    let mut collector = Collector::default();
    walk_program(program, &mut collector);
    collector.0
}
