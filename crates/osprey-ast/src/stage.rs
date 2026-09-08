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
use crate::{
    contains_resume, effect_name, walk_program, AstVisitor, Expr, HandlerArm, Multiplicity,
    Position, Program, Stmt,
};
use std::collections::BTreeMap;

pub use crate::stage_rows::dependencies;
pub(crate) use crate::stage_rows::{body_requirements, runtime_requirements};

/// The marker that declares a compile-time stage, spelled once so both flavors'
/// surfaces and every diagnostic agree. Implements [STAGE-DECL].
pub const STATIC_STAGE_KEYWORD: &str = "static";

/// The marker that opens a device-offload region, spelled once so the grammar,
/// both lowerers and every diagnostic agree. Implements [STAGE-GPU-KERNEL].
pub const KERNEL_REGION_KEYWORD: &str = "kernel";

/// When an effect's operations are answered. Implements [STAGE-AXIS].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Stage {
    /// Answered at runtime through the handler stack — every effect written
    /// without `static`, and the only stage Osprey had before staging.
    #[default]
    Dynamic,
    /// Answered by rewriting, before type checking. Implements [STAGE-DECL].
    Static,
    /// A `kernel` region: discharged exactly as [`Stage::Static`] is, and
    /// additionally obliged to leave a body whose residual DYNAMIC row is
    /// empty — the offload boundary cannot reach a runtime handler.
    /// Implements [STAGE-GPU-KERNEL], [STAGE-GPU-LEGAL].
    Kernel,
}

impl Stage {
    /// Whether the region is answered by rewriting rather than at runtime.
    /// A `kernel` is a static handler region that carries one extra obligation,
    /// so every phase that asks "is this discharged before the checker?" asks
    /// this rather than comparing against one variant. Implements [STAGE-LOWER].
    #[must_use]
    pub const fn is_compile_time(self) -> bool {
        matches!(self, Self::Static | Self::Kernel)
    }

    /// How a region of this stage is written, as diagnostics spell it.
    #[must_use]
    pub const fn region_keyword(self) -> &'static str {
        match self {
            Self::Dynamic => "handle",
            Self::Static => "handle static",
            Self::Kernel => KERNEL_REGION_KEYWORD,
        }
    }
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

/// How deeply one substitution may re-enter the rewrite. Discharge replaces a
/// `perform` with its arm's body and rewrites THAT, so a cycle between two
/// static handlers nests one native frame per step: the [`REWRITE_BOUND`] fuel
/// alone bounds how many steps run, not how deep they go, and 10,000 nested
/// frames overflow a worker thread's stack before the fuel is spent. This bound
/// stops the same divergence at a depth no legitimate program reaches — nesting
/// is how many answers are needed to reach ONE normal form, while the fuel
/// budget covers the whole program — and reports it as the same finite-rewrite
/// violation. Implements [STAGE-STATIC-FINITE].
pub(crate) const REWRITE_DEPTH_BOUND: u32 = 256;

/// The declared operations of one effect, with its stage.
pub(crate) struct EffectDecl {
    pub(crate) stage: Stage,
    operations: Vec<DeclaredOperation>,
    /// How many type parameters the declaration takes. An explicit mention
    /// must supply exactly this many, so `Signal<Count>` names an instantiation
    /// the declaration can actually have. Implements [STAGE-SIGNALS-EXACT].
    type_parameters: usize,
    position: Option<Position>,
}

/// One operation as its declaration wrote it. Multiplicity travels in the
/// summary so [MULTI-AXIS-STATIC] is decided by the pass that already knows
/// each effect's stage, rather than by a second walk over the same declarations.
struct DeclaredOperation {
    name: String,
    /// The keyword as WRITTEN. [MULTI-AXIS-STATIC] rejects a multiplicity
    /// written on a static operation, and `once` — the default — is a legal
    /// thing to write, so the annotation's presence is what this records.
    declared_multiplicity: Option<Multiplicity>,
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
    let declarations = effect_declarations(program);
    let mut errors = static_multiplicities(&declarations);
    errors.extend(validate_handlers(program, &declarations));
    // The offload obligation reads the row a WELL-FORMED region leaves behind,
    // so a region that broke a rule above would cascade a second, derived
    // complaint about the effect the first one already named.
    if errors.is_empty() {
        errors.extend(crate::kernel::legality(program));
    }
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
                type_params,
                position,
                ..
            } = statement
            {
                let declared = EffectDecl {
                    stage: *stage,
                    type_parameters: type_params.len(),
                    operations: operations
                        .iter()
                        .map(|op| DeclaredOperation {
                            name: op.name.clone(),
                            declared_multiplicity: op.declared_multiplicity,
                            position: op.position,
                        })
                        .collect(),
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
        match expression {
            Expr::Handler {
                stage,
                effect,
                arms,
                position,
                ..
            } => {
                self.errors.extend(instantiation_errors(
                    self.effects,
                    effect,
                    Site::region(*stage),
                    *position,
                ));
                self.register(*stage, effect, arms, *position);
            }
            Expr::Perform {
                effect, position, ..
            } => self.errors.extend(instantiation_errors(
                self.effects,
                effect,
                Site::REQUEST,
                *position,
            )),
            _ => {}
        }
    }
}

/// Where an effect mention appears: a `perform`, or a region of some stage.
/// The stage matters because a region whose stage disagrees with its effect's
/// is already rejected by that rule, and a second complaint about the same
/// mention would advise a spelling the first rule still forbids.
#[derive(Clone, Copy)]
struct Site {
    keyword: &'static str,
    stage: Option<Stage>,
}

impl Site {
    /// A `perform`, which has no stage of its own.
    const REQUEST: Self = Self {
        keyword: "perform",
        stage: None,
    };

    /// A handler region, written as its stage writes it.
    const fn region(stage: Stage) -> Self {
        Self {
            keyword: stage.region_keyword(),
            stage: Some(stage),
        }
    }

    /// Whether this site's stage can answer an effect declared at `declared`.
    /// A `perform` can be answered at either, so it never disagrees.
    fn agrees_with(self, declared: Stage) -> bool {
        self.stage
            .is_none_or(|stage| stage.is_compile_time() == (declared == Stage::Static))
    }
}

/// What an explicit instantiation at a mention site must satisfy.
///
/// Identity is the instantiation ([STAGE-SIGNALS-EXACT]), which is what makes a
/// dependency set exact — and which only the STATIC stage can represent: a
/// dynamic handler is keyed by effect name at runtime, so two instantiations of
/// one dynamic effect would share a key and the second would silently answer
/// the first. Saying so is the obligation; accepting it is the silent wrong
/// answer. Implements [STAGE-SIGNALS-EXACT].
fn instantiation_errors(
    effects: &BTreeMap<String, EffectDecl>,
    effect: &str,
    site: Site,
    position: Option<Position>,
) -> Vec<StageError> {
    if effect_name::instantiation(effect).is_none() {
        return Vec::new();
    }
    let base = effect_name::base(effect);
    let Some(declared) = effects.get(base) else {
        return Vec::new();
    };
    if !site.agrees_with(declared.stage) {
        return Vec::new();
    }
    let site = site.keyword;
    let supplied = effect_name::arity(effect);
    if declared.stage == Stage::Dynamic {
        return vec![StageError::new(
            format!(
                "`{site} {effect}` names an instantiation of dynamic effect `{base}`; a dynamic handler is keyed by effect name at runtime, so instantiations share one key and cannot be told apart (docs/plans/0024-staged-effects.md). Declare `static effect {base}`, or write `{site} {base}` and let inference instantiate it"
            ),
            position,
        )];
    }
    if supplied != declared.type_parameters {
        let expected = declared.type_parameters;
        let plural = if expected == 1 { "" } else { "s" };
        return vec![StageError::new(
            format!(
                "effect `{base}` takes {expected} type parameter{plural}; `{effect}` supplies {supplied}"
            ),
            position,
        )];
    }
    Vec::new()
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
        let base = effect_name::base(effect);
        let declared_stage = self.effects.get(base).map(|declared| declared.stage);
        match (stage, declared_stage) {
            (Stage::Kernel, Some(Stage::Dynamic)) => self.errors.push(StageError::new(
                format!(
                    "kernel arm names dynamic effect `{base}`; a kernel supplies the STATIC handlers for the device dialects its body uses, so declare `static effect {base}`"
                ),
                position,
            )),
            (Stage::Static, Some(Stage::Dynamic)) => self.errors.push(StageError::new(
                format!(
                    "effect `{base}` is dynamic; a static handler requires `static effect {base}`"
                ),
                position,
            )),
            (Stage::Dynamic, Some(Stage::Static)) => self.errors.push(StageError::new(
                format!(
                    "effect `{base}` is static; handle it with `handle static {base}` so it is discharged at compile time"
                ),
                position,
            )),
            (Stage::Static | Stage::Kernel, None) => self.errors.push(StageError::new(
                format!("{} names unknown effect `{base}`", stage.region_keyword()),
                position,
            )),
            (Stage::Static | Stage::Kernel, Some(_)) => {
                self.register_static(effect, arms, position);
            }
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
    let Some(declared) = effects.get(effect_name::base(effect)) else {
        return Vec::new();
    };
    declared
        .operations
        .iter()
        .filter(|operation| !arms.iter().any(|arm| arm.operation == operation.name))
        .map(|operation| {
            let name = &operation.name;
            StageError::new(
                format!("static handler for `{effect}` does not cover operation `{effect}.{name}`"),
                position.or(declared.position),
            )
        })
        .collect()
}

/// Static effects sit outside the multiplicity lattice: [STAGE-STATIC-TAIL]
/// pins them at exactly-once-in-tail-position, the one point where a
/// continuation need not exist, so a multiplicity written on one is not a
/// narrowing but a contradiction. Implements [MULTI-AXIS-STATIC].
fn static_multiplicities(effects: &BTreeMap<String, EffectDecl>) -> Vec<StageError> {
    effects
        .iter()
        .filter(|(_, declared)| declared.stage == Stage::Static)
        .flat_map(|(effect, declared)| {
            declared
                .operations
                .iter()
                .filter(|operation| operation.declared_multiplicity.is_some())
                .map(move |operation| {
                    let name = &operation.name;
                    StageError::new(
                        format!("multiplicity on static effect `{effect}.{name}`; static operations are always tail-resumptive"),
                        operation.position.or(declared.position),
                    )
                })
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
                .get(effect_name::base(performed))
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
