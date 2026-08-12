//! The static-handler rewrite: one lowering pass per `handle static` region.
//!
//! A region answers its effect for its own body *and* for every function that
//! body reaches, so two regions may answer the same effect differently. Each
//! region therefore specializes the helpers it reaches — `tiled` under a region
//! answering `Tile.size => 8` becomes a distinct definition from `tiled` under
//! a region answering `Tile.size => 3`.
//!
//! The traversal is top-down over a stack of enclosing regions, so nested
//! regions compose: a helper reached under two regions is specialized once
//! against both, and a `perform` resolves to the innermost region answering its
//! effect. Implements [STAGE-LOWER], [STAGE-LOWER-ORDER].

use crate::mutate::{children_mut, statement_children_mut};
use crate::stage::{dependencies, StageError, REWRITE_BOUND, REWRITE_DEPTH_BOUND};
use crate::{Expr, HandlerArm, Position, Program, Stage, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// One `handle static` region on the enclosing stack.
struct Region {
    effect: String,
    arms: Vec<HandlerArm>,
    id: u32,
}

impl Region {
    fn clone_region(&self) -> Self {
        Self {
            effect: self.effect.clone(),
            arms: self.arms.clone(),
            id: self.id,
        }
    }
}

/// The pass state: the program's function definitions plus what the rewrite has
/// produced so far.
struct Lowering {
    /// Every named function's definition, keyed by name.
    definitions: BTreeMap<String, Stmt>,
    /// Each function's transitive static-operation requirements.
    requirements: BTreeMap<String, Vec<String>>,
    /// Region-specialized definitions appended to the program.
    produced: Vec<Stmt>,
    /// Specialization cache, keyed by function name and enclosing region ids.
    emitted: BTreeMap<String, String>,
    /// Names whose original definition the regions consumed.
    consumed: BTreeSet<String>,
    errors: Vec<StageError>,
    regions: u32,
    fuel: u32,
    /// How many substitutions are currently nested, so a diverging rewrite is
    /// reported rather than overflowing the stack ([`REWRITE_DEPTH_BOUND`]).
    depth: u32,
}

/// Discharge every static handler in `program`.
pub(crate) fn run(program: &Program) -> Result<Program, Vec<StageError>> {
    let mut lowering = Lowering {
        definitions: function_definitions(program),
        requirements: dependencies(program),
        produced: Vec::new(),
        emitted: BTreeMap::new(),
        consumed: BTreeSet::new(),
        errors: Vec::new(),
        regions: 0,
        fuel: REWRITE_BOUND,
        depth: 0,
    };
    let mut rewritten = program.clone();
    for statement in &mut rewritten.statements {
        statement_children_mut(statement, &mut |expression| {
            lowering.rewrite(expression, &mut Vec::new());
        });
    }
    rewritten
        .statements
        .retain(|statement| !lowering.consumed_definition(statement));
    rewritten.statements.extend(lowering.produced);
    if lowering.errors.is_empty() {
        Ok(rewritten)
    } else {
        Err(lowering.errors)
    }
}

impl Lowering {
    /// A function whose only legal callers were inside the regions that
    /// specialized it: those regions own it now.
    fn consumed_definition(&self, statement: &Stmt) -> bool {
        matches!(statement, Stmt::Function { name, .. } if self.consumed.contains(name))
    }

    /// Rewrite one expression under the regions enclosing it.
    fn rewrite(&mut self, expression: &mut Expr, regions: &mut Vec<Region>) {
        match expression {
            Expr::Handler {
                stage: Stage::Static,
                ..
            } => self.enter_region(expression, regions),
            Expr::Perform { effect, .. } if answered(regions, effect) => {
                self.substitute(expression, regions);
            }
            Expr::Identifier(name) => {
                let referenced = name.clone();
                if let Some(specialized) = self.specialize(&referenced, regions) {
                    name.clone_from(&specialized);
                }
            }
            _ => children_mut(expression, &mut |child| self.rewrite(child, regions)),
        }
    }

    /// Discharge one region: its body is rewritten with the region pushed, and
    /// the handler node is replaced by that body. Implements [STAGE-RESIDUE].
    fn enter_region(&mut self, expression: &mut Expr, regions: &mut Vec<Region>) {
        let Expr::Handler {
            effect, arms, body, ..
        } = expression
        else {
            return;
        };
        self.regions = self.regions.saturating_add(1);
        let region = Region {
            effect: effect.clone(),
            arms: arms.clone(),
            id: self.regions,
        };
        let mut discharged = (**body).clone();
        regions.push(region);
        self.rewrite(&mut discharged, regions);
        let _ = regions.pop();
        *expression = discharged;
    }

    /// Replace one performed operation with the innermost answering arm.
    fn substitute(&mut self, expression: &mut Expr, regions: &mut Vec<Region>) {
        children_mut(expression, &mut |child| self.rewrite(child, regions));
        let Expr::Perform {
            effect,
            operation,
            arguments,
            position,
            ..
        } = expression
        else {
            return;
        };
        let (effect, operation) = (effect.clone(), operation.clone());
        let (arguments, position) = (arguments.clone(), *position);
        let Some(arm) = innermost_arm(regions, &effect, &operation) else {
            self.errors.push(StageError::new(
                format!(
                    "static handler for `{effect}` does not cover operation `{effect}.{operation}`"
                ),
                position,
            ));
            return;
        };
        if arm.params.len() != arguments.len() {
            self.errors.push(arity_error(
                &effect,
                &operation,
                &arm,
                arguments.len(),
                position,
            ));
            return;
        }
        if self.spend_fuel(&effect, &operation, position) {
            *expression = bind_parameters(&arm, &arguments);
            self.depth = self.depth.saturating_add(1);
            self.rewrite(expression, regions);
            self.depth = self.depth.saturating_sub(1);
        }
    }

    /// Consume one rewrite step. Implements [STAGE-STATIC-FINITE].
    fn spend_fuel(&mut self, effect: &str, operation: &str, position: Option<Position>) -> bool {
        if let Some(remaining) = self
            .fuel
            .checked_sub(1)
            .filter(|_| self.depth < REWRITE_DEPTH_BOUND)
        {
            self.fuel = remaining;
            true
        } else {
            self.errors.push(StageError::new(
                format!("static discharge of `{effect}.{operation}` exceeded the rewrite bound ({REWRITE_BOUND} steps)"),
                position,
            ));
            false
        }
    }

    /// The name a reference resolves to under `regions`: a region-owned copy
    /// when the referenced function requires an effect the regions answer, and
    /// nothing when it does not.
    fn specialize(&mut self, name: &str, regions: &[Region]) -> Option<String> {
        if !self.reaches_answered_effect(name, regions) {
            return None;
        }
        let key = specialization_key(name, regions);
        if let Some(existing) = self.emitted.get(&key) {
            return Some(existing.clone());
        }
        let specialized = format!("{name}__stage{}", self.emitted.len().saturating_add(1));
        let _ = self.emitted.insert(key, specialized.clone());
        self.produce(name, &specialized, regions);
        Some(specialized)
    }

    /// Whether `name` is a function requiring an operation these regions answer.
    fn reaches_answered_effect(&self, name: &str, regions: &[Region]) -> bool {
        self.requirements.get(name).is_some_and(|operations| {
            operations
                .iter()
                .any(|operation| regions.iter().any(|region| owns(&region.effect, operation)))
        })
    }

    /// Emit one region-owned copy of a function, rewritten under the regions.
    fn produce(&mut self, original: &str, specialized: &str, regions: &[Region]) {
        let Some(Stmt::Function {
            type_params,
            parameters,
            return_type,
            effects,
            body,
            doc,
            position,
            ..
        }) = self.definitions.get(original).cloned()
        else {
            return;
        };
        let mut owned: Vec<Region> = regions.iter().map(Region::clone_region).collect();
        let mut specialized_body = body;
        self.rewrite(&mut specialized_body, &mut owned);
        let _ = self.consumed.insert(original.to_owned());
        self.produced.push(Stmt::Function {
            name: specialized.to_owned(),
            type_params,
            parameters,
            return_type,
            // The regions answered these effects, so the copy no longer
            // declares them. Implements [STAGE-RESIDUE].
            effects: effects
                .into_iter()
                .filter(|declared| !regions.iter().any(|region| region.effect == declared.name))
                .collect(),
            body: specialized_body,
            doc,
            position,
        });
    }
}

fn arity_error(
    effect: &str,
    operation: &str,
    arm: &HandlerArm,
    supplied: usize,
    position: Option<Position>,
) -> StageError {
    StageError::new(
        format!(
            "static handler arm `{effect}.{operation}` binds {} parameters but the operation is performed with {supplied} arguments",
            arm.params.len()
        ),
        position,
    )
}

/// Whether any enclosing region answers `effect`.
fn answered(regions: &[Region], effect: &str) -> bool {
    regions.iter().any(|region| region.effect == effect)
}

/// The innermost arm answering one operation.
fn innermost_arm(regions: &[Region], effect: &str, operation: &str) -> Option<HandlerArm> {
    regions
        .iter()
        .rev()
        .filter(|region| region.effect == effect)
        .find_map(|region| {
            region
                .arms
                .iter()
                .find(|arm| arm.operation == operation)
                .cloned()
        })
}

/// A specialization is identified by the function and the exact region stack it
/// was reached under.
fn specialization_key(name: &str, regions: &[Region]) -> String {
    let ids: Vec<String> = regions.iter().map(|region| region.id.to_string()).collect();
    format!("{name}|{}", ids.join("."))
}

/// Whether `effect` declares `operation` (an `Effect.op` name).
fn owns(effect: &str, operation: &str) -> bool {
    operation.split('.').next() == Some(effect)
}

/// The arm body with the operation's arguments bound to the arm's parameters.
/// A nullary operation needs no block, which keeps the common reactive read
/// ([STAGE-SIGNALS]) a bare expression.
fn bind_parameters(arm: &HandlerArm, arguments: &[Expr]) -> Expr {
    if arm.params.is_empty() {
        return arm.body.clone();
    }
    Expr::Block {
        statements: arm
            .params
            .iter()
            .zip(arguments)
            .map(|(name, value)| Stmt::Let {
                name: name.clone(),
                mutable: false,
                ty: None,
                value: value.clone(),
                doc: None,
                position: None,
            })
            .collect(),
        value: Some(Box::new(arm.body.clone())),
    }
}

/// Every named function definition in the program, at any nesting depth.
fn function_definitions(program: &Program) -> BTreeMap<String, Stmt> {
    #[derive(Default)]
    struct Collector(BTreeMap<String, Stmt>);
    impl crate::AstVisitor for Collector {
        fn statement(&mut self, statement: &Stmt) {
            if let Stmt::Function { name, .. } = statement {
                let _ = self.0.insert(name.clone(), statement.clone());
            }
        }
    }
    let mut collector = Collector::default();
    crate::walk_program(program, &mut collector);
    collector.0
}
