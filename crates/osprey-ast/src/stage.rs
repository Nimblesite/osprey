//! Staged effect discharge — a static handler is a lowering pass.
//!
//! An effect declared `static` is a compile-time dialect: its operations are
//! rewritten away after its data contracts are checked, so nothing of it
//! survives into code generation ([STAGE-RESIDUE]). This module is that
//! rewrite over the canonical AST produced by either flavor.
//!
//! Running before residual effect validation means a kernel body whose
//! requests were all answered here arrives at
//! [GPU-KERNEL-PURE] with an empty row and passes the purity gate unchanged,
//! and a `wasm32` build never sees an operation needing a continuation.
//!
//! Regions specialize the helpers they reach and compose innermost-first
//! ([STAGE-LOWER-ORDER]). Callers must validate the source contracts before
//! invoking this rewrite; the public compiler entry is
//! `osprey_types::lower_static_checked`.

use crate::{
    effect_name, walk_program, AstVisitor, Expr, HandlerArm, OperationMode, Position, Program, Stmt,
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
    /// Default runtime interpretation. Unmarked value effects also permit an
    /// explicitly selected static interpretation.
    #[default]
    Dynamic,
    /// Answered by rewriting after source type validation. Implements [STAGE-DECL].
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
    /// so every phase that asks whether a region is discharged before runtime asks
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
    pub(crate) operations: Vec<DeclaredOperation>,
    /// How many type parameters the declaration takes. An explicit mention
    /// must supply exactly this many, so `Signal<Count>` names an instantiation
    /// the declaration can actually have. Implements [STAGE-SIGNALS-EXACT].
    type_parameters: usize,
    position: Option<Position>,
}

/// One operation as its declaration wrote it. Mode travels in the summary so
/// [MULTI-AXIS-STATIC] and [STAGE-STATIC-TOTAL] are decided by the pass that
/// already knows each effect's stage, rather than by a second walk over the
/// same declarations.
pub(crate) struct DeclaredOperation {
    pub(crate) name: String,
    /// Whether `control` was written. A control operation has a continuation,
    /// which is exactly what a static interpretation cannot provide.
    mode: OperationMode,
    position: Option<Position>,
}

/// Every violated staging rule in `program`, without rewriting it: a modifier
/// a declaration cannot carry ([MULTI-DECL]), a stage mismatch between an
/// effect and its handler, a partial static handler ([STAGE-STATIC-TOTAL]), a
/// continuation capture ([STAGE-STATIC-TAIL]), a malformed instantiation
/// ([EFFECTS-GENERIC-DECL]) or an offload boundary with a residual request
/// ([STAGE-GPU-LEGAL]). These read declarations and syntax only, so they run
/// before type inference and speak first: a residual row would otherwise
/// report the same defect as a bare unhandled operation.
#[must_use]
pub fn validate(program: &Program) -> Vec<StageError> {
    let declarations = effect_declarations(program);
    let mut errors = modifier_errors(program);
    errors.extend(validate_handlers(program, &declarations));
    // The offload obligation reads the row a WELL-FORMED region leaves behind,
    // so a region that broke a rule above would cascade a second, derived
    // complaint about the effect the first one already named.
    if errors.is_empty() {
        errors.extend(crate::kernel::legality(program));
    }
    errors
}

/// Rewrite every static handler in a program [`validate`] accepted.
/// Implements [STAGE-LOWER].
///
/// # Errors
/// A runtime request from a compile-time answer ([STAGE-STATIC-MONOTONE]) or
/// an unbounded rewrite ([STAGE-STATIC-FINITE]).
pub fn lower(program: &Program) -> Result<Program, Vec<StageError>> {
    crate::lower_static::run(program)
}

/// [`validate`] then [`lower`]: discharge every static handler in `program`,
/// returning the rewritten program.
///
/// # Errors
/// Every violated staging rule, from either phase.
pub fn discharge(program: &Program) -> Result<Program, Vec<StageError>> {
    let errors = validate(program);
    if !errors.is_empty() {
        return Err(errors);
    }
    lower(program)
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
                            mode: op.mode,
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
                self.errors
                    .extend(instantiation_errors(self.effects, effect, *position));
                self.register(*stage, effect, arms, *position);
            }
            Expr::Perform {
                effect, position, ..
            } => self
                .errors
                .extend(instantiation_errors(self.effects, effect, *position)),
            _ => {}
        }
    }
}

/// Explicit instantiation identifies the same operation at either stage.
fn instantiation_errors(
    effects: &BTreeMap<String, EffectDecl>,
    effect: &str,
    position: Option<Position>,
) -> Vec<StageError> {
    if effect_name::instantiation(effect).is_none() {
        return Vec::new();
    }
    let base = effect_name::base(effect);
    let Some(declared) = effects.get(base) else {
        return Vec::new();
    };
    let supplied = effect_name::arity(effect);
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
    // The DECLARATION decides, never a search of an arm's body for `resume`: a
    // control operation owns a continuation, which is precisely what an answer
    // computed at compile time cannot supply. Every operation of the region's
    // effect is checked, covered by an arm or not. Implements
    // [STAGE-STATIC-TOTAL], [STAGE-STATIC-TAIL].
    if let Some(declared) = effects.get(effect_name::base(effect)) {
        for operation in &declared.operations {
            if operation.mode.is_control() {
                errors.push(StageError::new(
                    format!("static interpretation of `{effect}` requires all value operations; `{effect}.{}` is a control operation", operation.name),
                    position.or(operation.position),
                ));
            }
        }
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

/// A static interpretation answers an operation at compile time and has no
/// continuation to give away, so a `control` operation cannot be one:
/// [STAGE-STATIC-TAIL] pins static arms at exactly-once-in-tail-position.
/// Implements [MULTI-AXIS-STATIC], [STAGE-STATIC-TOTAL].
/// The modifier defect of every declared operation, by the one rule the type
/// checker applies too ([`crate::EffectOperation::modifier_error`]).
fn modifier_errors(program: &Program) -> Vec<StageError> {
    #[derive(Default)]
    struct Modifiers(Vec<StageError>);
    impl AstVisitor for Modifiers {
        fn statement(&mut self, statement: &Stmt) {
            let Stmt::Effect {
                name,
                stage,
                operations,
                position,
                ..
            } = statement
            else {
                return;
            };
            self.0.extend(operations.iter().filter_map(|operation| {
                operation
                    .modifier_error(name, *stage)
                    .map(|message| StageError::new(message, operation.position.or(*position)))
            }));
        }
    }
    let mut modifiers = Modifiers::default();
    walk_program(program, &mut modifiers);
    modifiers.0
}
