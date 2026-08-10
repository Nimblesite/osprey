//! Staged effect discharge — a static handler is a lowering pass.
//!
//! An effect declared `static` is a compile-time dialect: its operations are
//! rewritten away before type checking, so nothing of it survives into code
//! generation ([STAGE-RESIDUE]). This module is that rewrite. It runs on the
//! canonical AST both flavors lower to, which is why staging costs no new
//! machinery in the checker, the backend or the runtime — after the pass, a
//! program that used static effects *is* an ordinary program.
//!
//! Running before the checker is what makes the four payoffs fall out of one
//! mechanism: a kernel body whose requests were all answered here arrives at
//! [GPU-KERNEL-PURE] with an empty row and passes the purity gate unchanged,
//! and a `wasm32` build never sees an operation needing a continuation.
//!
//! Prototype scope ([STAGE-PROTO-WHOLE-PROGRAM]): one static handler per static
//! effect per program, applied program-wide rather than per lexical region. The
//! spec's per-region rule ([STAGE-LOWER-ORDER]) is a superset of this.

use crate::mutate::children_mut;
use crate::{contains_resume, walk_program, AstVisitor, Expr, HandlerArm, Position, Program, Stmt};
use std::collections::BTreeMap;

/// When an effect's operations are answered. Implements [STAGE-AXIS].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Stage {
    /// Answered at runtime through the handler stack — every effect written
    /// without `static`, and the only stage Osprey had before staging.
    #[default]
    Dynamic,
    /// Answered by rewriting, before type checking. Implements [STAGE-DECL].
    Static,
}

/// One violated staging rule, reported like any other compile error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageError {
    /// Human-readable rejection naming the effect and operation.
    pub message: String,
    /// Source position of the offending declaration, handler or operation.
    pub position: Option<Position>,
}

impl StageError {
    pub(crate) fn new(message: impl Into<String>, position: Option<Position>) -> Self {
        Self {
            message: message.into(),
            position,
        }
    }
}

/// Substitution steps allowed before the rewrite is declared non-terminating.
/// Implements [STAGE-STATIC-FINITE].
pub(crate) const REWRITE_BOUND: u32 = 10_000;

/// The declared operations of one effect, with its stage.
pub(crate) struct EffectDecl {
    pub(crate) stage: Stage,
    operations: Vec<String>,
    position: Option<Position>,
}

/// Discharge every static handler in `program`, returning the rewritten
/// program. Implements [STAGE-LOWER].
///
/// # Errors
/// Returns every violated staging rule: a stage mismatch between an effect and
/// its handler, a partial static handler ([STAGE-STATIC-TOTAL]), a continuation
/// capture ([STAGE-STATIC-TAIL]), a runtime request from a compile-time answer
/// ([STAGE-STATIC-MONOTONE]) or an unbounded rewrite ([STAGE-STATIC-FINITE]).
pub fn discharge(program: &Program) -> Result<Program, Vec<StageError>> {
    let errors = validate_handlers(program, &effect_declarations(program));
    if !errors.is_empty() {
        return Err(errors);
    }
    crate::lower_static::run(program)
}

/// Index every `effect` declaration, at any nesting depth, by name.
pub(crate) fn effect_declarations(program: &Program) -> BTreeMap<String, EffectDecl> {
    #[derive(Default)]
    struct Collector(BTreeMap<String, EffectDecl>);
    impl AstVisitor for Collector {
        fn statement(&mut self, statement: &Stmt) {
            if let Stmt::Effect {
                stage,
                name,
                operations,
                position,
                ..
            } = statement
            {
                let declared = EffectDecl {
                    stage: *stage,
                    operations: operations.iter().map(|op| op.name.clone()).collect(),
                    position: *position,
                };
                let _ = self.0.insert(name.clone(), declared);
            }
        }
    }
    let mut collector = Collector::default();
    walk_program(program, &mut collector);
    collector.0
}

/// Check every handler in the program against its effect's stage, and every
/// static handler against the four obligations.
fn validate_handlers(program: &Program, effects: &BTreeMap<String, EffectDecl>) -> Vec<StageError> {
    let mut collector = RuleCollector {
        effects,
        errors: Vec::new(),
    };
    walk_program(program, &mut collector);
    collector.errors
}

struct RuleCollector<'a> {
    effects: &'a BTreeMap<String, EffectDecl>,
    errors: Vec<StageError>,
}

impl AstVisitor for RuleCollector<'_> {
    fn expression(&mut self, expression: &Expr) {
        if let Expr::Handler {
            stage,
            effect,
            arms,
            position,
            ..
        } = expression
        {
            self.register(*stage, effect, arms, *position);
        }
    }
}

impl RuleCollector<'_> {
    /// Validate one handler against its effect's stage and record its arms when
    /// the handler is static.
    fn register(
        &mut self,
        stage: Stage,
        effect: &str,
        arms: &[HandlerArm],
        position: Option<Position>,
    ) {
        let declared_stage = self.effects.get(effect).map(|declared| declared.stage);
        match (stage, declared_stage) {
            (Stage::Static, Some(Stage::Dynamic)) => self.errors.push(StageError::new(
                format!(
                    "effect `{effect}` is dynamic; a static handler requires `static effect {effect}`"
                ),
                position,
            )),
            (Stage::Dynamic, Some(Stage::Static)) => self.errors.push(StageError::new(
                format!(
                    "effect `{effect}` is static; handle it with `handle static {effect}` so it is discharged at compile time"
                ),
                position,
            )),
            (Stage::Static, None) => self.errors.push(StageError::new(
                format!("static handler names unknown effect `{effect}`"),
                position,
            )),
            (Stage::Static, Some(Stage::Static)) => self.register_static(effect, arms, position),
            (Stage::Dynamic, _) => {}
        }
    }

    fn register_static(&mut self, effect: &str, arms: &[HandlerArm], position: Option<Position>) {
        self.errors
            .extend(validate_static_arms(self.effects, effect, arms, position));
    }
}

/// The three obligations a static handler's arms must meet.
pub(crate) fn validate_static_arms(
    effects: &BTreeMap<String, EffectDecl>,
    effect: &str,
    arms: &[HandlerArm],
    position: Option<Position>,
) -> Vec<StageError> {
    let mut errors = missing_arms(effects, effect, arms, position);
    for arm in arms {
        if contains_resume(&arm.body) {
            errors.push(StageError::new(
                format!(
                    "static handler arm `{effect}.{}` resumes; static handlers cannot capture a continuation",
                    arm.operation
                ),
                arm.position.or(position),
            ));
        }
        errors.extend(dynamic_requests(effects, effect, arm));
    }
    errors
}

/// Implements [STAGE-STATIC-TOTAL].
fn missing_arms(
    effects: &BTreeMap<String, EffectDecl>,
    effect: &str,
    arms: &[HandlerArm],
    position: Option<Position>,
) -> Vec<StageError> {
    let Some(declared) = effects.get(effect) else {
        return Vec::new();
    };
    declared
        .operations
        .iter()
        .filter(|operation| !arms.iter().any(|arm| &&arm.operation == operation))
        .map(|operation| {
            StageError::new(
                format!(
                    "static handler for `{effect}` does not cover operation `{effect}.{operation}`"
                ),
                position.or(declared.position),
            )
        })
        .collect()
}

/// Implements [STAGE-STATIC-MONOTONE].
fn dynamic_requests(
    effects: &BTreeMap<String, EffectDecl>,
    effect: &str,
    arm: &HandlerArm,
) -> Vec<StageError> {
    performed_effects(&arm.body)
        .into_iter()
        .filter(|(performed, _)| {
            effects
                .get(performed)
                .is_none_or(|declared| declared.stage == Stage::Dynamic)
        })
        .map(|(performed, operation)| {
            StageError::new(
                format!(
                    "static handler arm `{effect}.{}` requires dynamic effect `{performed}.{operation}`; static handler arms may require only static effects",
                    arm.operation
                ),
                arm.position,
            )
        })
        .collect()
}

/// Every `perform` reachable inside one expression, as effect/operation pairs.
pub(crate) fn performed_effects(expression: &Expr) -> Vec<(String, String)> {
    let mut found = Vec::new();
    collect_performs(&mut expression.clone(), &mut found);
    found
}

fn collect_performs(expression: &mut Expr, found: &mut Vec<(String, String)>) {
    if let Expr::Perform {
        effect, operation, ..
    } = expression
    {
        found.push((effect.clone(), operation.clone()));
    }
    children_mut(expression, &mut |child| collect_performs(child, found));
}

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
    let effects = effect_declarations(program);
    let facts: BTreeMap<String, BodyFacts> = function_bodies(program)
        .iter()
        .map(|(name, body)| (name.clone(), body_facts(body, &effects)))
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

/// One fixed-point step: each function gains every requirement of what it
/// references, except the effects a region already answered around it.
fn propagate(
    facts: &BTreeMap<String, BodyFacts>,
    required: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<String>> {
    facts
        .iter()
        .map(|(name, fact)| {
            let mut operations = fact.direct.clone();
            for (callee, handled) in &fact.references {
                let inherited = required.get(callee).into_iter().flatten();
                operations.extend(
                    inherited
                        .filter(|operation| !handled.iter().any(|e| owns(e, operation)))
                        .cloned(),
                );
            }
            (name.clone(), sorted(operations))
        })
        .collect()
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
fn body_facts(body: &Expr, effects: &BTreeMap<String, EffectDecl>) -> BodyFacts {
    let mut facts = BodyFacts {
        direct: Vec::new(),
        references: Vec::new(),
    };
    scan_body(&mut body.clone(), effects, &mut Vec::new(), &mut facts);
    facts.direct = sorted(std::mem::take(&mut facts.direct));
    facts
}

fn scan_body(
    expression: &mut Expr,
    effects: &BTreeMap<String, EffectDecl>,
    handled: &mut Vec<String>,
    facts: &mut BodyFacts,
) {
    match expression {
        Expr::Handler {
            stage: Stage::Static,
            effect,
            arms,
            body,
            ..
        } => {
            let effect = effect.clone();
            for arm in arms {
                scan_body(&mut arm.body, effects, handled, facts);
            }
            handled.push(effect);
            scan_body(body, effects, handled, facts);
            let _ = handled.pop();
        }
        Expr::Perform {
            effect, operation, ..
        } => {
            let (effect, operation) = (effect.clone(), operation.clone());
            if is_static(effects, &effect) && !handled.contains(&effect) {
                facts.direct.push(format!("{effect}.{operation}"));
            }
            children_mut(expression, &mut |child| {
                scan_body(child, effects, handled, facts);
            });
        }
        Expr::Identifier(name) => facts.references.push((name.clone(), handled.clone())),
        _ => children_mut(expression, &mut |child| {
            scan_body(child, effects, handled, facts);
        }),
    }
}

fn is_static(effects: &BTreeMap<String, EffectDecl>, effect: &str) -> bool {
    effects
        .get(effect)
        .is_some_and(|declared| declared.stage == Stage::Static)
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
