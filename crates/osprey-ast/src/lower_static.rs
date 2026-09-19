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
use crate::stage::{
    effect_declarations, EffectDecl, StageError, REWRITE_BOUND, REWRITE_DEPTH_BOUND,
};
use crate::{Expr, HandlerArm, Parameter, Position, Program, Stmt};
use std::collections::{BTreeMap, BTreeSet};

mod bindings;
mod hygiene;
mod requirements;
use requirements::{all_requirements, function_definitions, retain_referenced_originals};

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
    bindings: BTreeMap<String, Expr>,
    effects: BTreeMap<String, EffectDecl>,
    /// Each function's requirements, independent of its selected interpretation.
    requirements: BTreeMap<String, Vec<String>>,
    dispatches: BTreeMap<String, Vec<String>>,
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
    if !has_regions(program) {
        return Ok(program.clone());
    }
    let program = hygiene::resolve(program);
    let mut lowering = Lowering {
        definitions: function_definitions(&program),
        bindings: bindings::collect(&program),
        effects: effect_declarations(&program),
        requirements: all_requirements(&program),
        dispatches: crate::stage_rows::dispatch_requirements(&program),
        produced: Vec::new(),
        emitted: BTreeMap::new(),
        consumed: BTreeSet::new(),
        errors: Vec::new(),
        regions: 0,
        fuel: REWRITE_BOUND,
        depth: 0,
    };
    let mut rewritten = program;
    for statement in &mut rewritten.statements {
        statement_children_mut(statement, &mut |expression| {
            lowering.rewrite(expression, &mut Vec::new());
        });
    }
    rewritten.statements.extend(lowering.produced);
    retain_referenced_originals(&mut rewritten, &lowering.consumed);
    if lowering.errors.is_empty() {
        Ok(rewritten)
    } else {
        Err(lowering.errors)
    }
}

fn has_regions(program: &Program) -> bool {
    #[derive(Default)]
    struct Regions(bool);
    impl crate::AstVisitor for Regions {
        fn expression(&mut self, expression: &Expr) {
            self.0 |= matches!(expression, Expr::Handler { stage, .. } if stage.is_compile_time());
        }
    }
    let mut regions = Regions::default();
    crate::walk_program(program, &mut regions);
    regions.0
}

impl Lowering {
    /// Rewrite one expression under the regions enclosing it.
    fn rewrite(&mut self, expression: &mut Expr, regions: &mut Vec<Region>) {
        if self.apply_static_handler(expression) {
            self.depth = self.depth.saturating_add(1);
            self.rewrite(expression, regions);
            self.depth = self.depth.saturating_sub(1);
            return;
        }
        match expression {
            // A `kernel` is a static handler region carrying one extra
            // obligation, already discharged by `kernel::legality`; the rewrite
            // that answers it is the same one. Implements [STAGE-GPU-KERNEL].
            Expr::Handler { stage, .. } if stage.is_compile_time() => {
                self.enter_region(expression, regions);
            }
            Expr::Handler {
                effect,
                arms,
                body,
                return_clause,
                ..
            } => {
                // Dynamic selection shadows the same effect's static selection.
                // Arms execute outside this activation and retain outer scopes.
                for arm in arms {
                    self.rewrite(&mut arm.body, regions);
                }
                if let Some(clause) = return_clause {
                    self.rewrite(clause, regions);
                }
                let mut visible: Vec<Region> = regions
                    .iter()
                    .filter(|region| region.effect != *effect)
                    .map(Region::clone_region)
                    .collect();
                self.rewrite(body, &mut visible);
            }
            Expr::Perform { effect, .. } if answered(regions, effect) => {
                self.substitute(expression, regions);
            }
            Expr::Identifier(name) => {
                let referenced = name.clone();
                if let Some(specialized) = self.specialize(&referenced, regions) {
                    *expression = specialized;
                }
            }
            _ => children_mut(expression, &mut |child| self.rewrite(child, regions)),
        }
    }

    /// Discharge one region: its body is rewritten with the region pushed, and
    /// the handler node is replaced by that body. Implements [STAGE-RESIDUE].
    fn enter_region(&mut self, expression: &mut Expr, regions: &mut Vec<Region>) {
        let Expr::Handler {
            effect,
            arms,
            body,
            return_clause,
            ..
        } = expression
        else {
            return;
        };
        self.regions = self.regions.saturating_add(1);
        let mut selected_arms = arms.clone();
        for arm in &mut selected_arms {
            // Validate unused arms too, and resolve their requests in the
            // enclosing scope rather than recursively selecting themselves.
            self.rewrite(&mut arm.body, regions);
            let mut required = self.remaining_requirements(&arm.body);
            required.extend(crate::stage_rows::body_dispatches(
                &self.effects,
                &arm.body,
                &self.dispatches,
            ));
            required.sort();
            required.dedup();
            if !required.is_empty() {
                self.errors.push(StageError::new(
                    format!("static handler arm `{effect}.{}` requires runtime effects: {}; arm operations must be statically discharged", arm.operation, required.join(", ")),
                    arm.position,
                ));
            }
        }
        let region = Region {
            effect: effect.clone(),
            arms: selected_arms,
            id: self.regions,
        };
        let mut discharged = (**body).clone();
        regions.push(region);
        self.rewrite(&mut discharged, regions);
        let _ = regions.pop();
        *expression = if let Some(mut clause) = return_clause.take() {
            self.rewrite(&mut clause, regions);
            Expr::Call {
                function: clause,
                arguments: vec![discharged],
                named_arguments: Vec::new(),
            }
        } else {
            discharged
        };
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
        // A region that answers nothing here, or an arm whose parameters do not
        // match the request, is already rejected before the rewrite runs:
        // `stage::validate` names the uncovered operation and the type checker
        // names the arity ([STAGE-LOWER-ORDER-PHASE]). Repeating either
        // diagnostic here would only give one defect two wordings, so an
        // unrewritable request is left standing for the residual row to report.
        let Some((region_index, arm)) = innermost_arm(regions, &effect, &operation)
            .filter(|(_, arm)| arm.params.len() == arguments.len())
        else {
            return;
        };
        if self.spend_fuel(&effect, &operation, position) {
            *expression = bind_parameters(&arm, &arguments);
            self.depth = self.depth.saturating_add(1);
            let mut outer: Vec<Region> = regions
                .get(..region_index)
                .unwrap_or_default()
                .iter()
                .map(Region::clone_region)
                .collect();
            self.rewrite(expression, &mut outer);
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
    fn specialize(&mut self, name: &str, regions: &[Region]) -> Option<Expr> {
        if !self.reaches_answered_effect(name, regions) {
            return None;
        }
        if let Some(value) = self.specialize_binding(name, regions) {
            return Some(value);
        }
        let key = specialization_key(name, regions);
        if let Some(existing) = self.emitted.get(&key) {
            return self.specialized_reference(name, existing, regions);
        }
        let specialized = format!("{name}__stage{}", self.emitted.len().saturating_add(1));
        let _ = self.emitted.insert(key, specialized.clone());
        self.produce(name, &specialized, regions);
        self.specialized_reference(name, &specialized, regions)
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
            mut parameters,
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
        for region in &mut owned {
            for arm in &mut region.arms {
                if captures_lexical_values(arm) {
                    let name = callback_name(region.id, &arm.operation);
                    parameters.push(parameter(&name));
                    arm.body = call(
                        &name,
                        arm.params
                            .iter()
                            .map(|name| Expr::Identifier(name.clone()))
                            .collect(),
                    );
                }
            }
        }
        let mut specialized_body = body;
        self.rewrite(&mut specialized_body, &mut owned);
        let required = self.remaining_requirements(&specialized_body);
        let dispatches =
            crate::stage_rows::body_dispatches(&self.effects, &specialized_body, &self.dispatches);
        let _ = self.dispatches.insert(specialized.to_owned(), dispatches);
        let _ = self.requirements.insert(specialized.to_owned(), required);
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
                .filter(|declared| {
                    !regions.iter().any(|region| {
                        region.effect
                            == crate::effect_name::instantiated(&declared.name, &declared.type_args)
                    })
                })
                .collect(),
            body: specialized_body,
            doc,
            position,
        });
    }

    /// A specialization receives capturing arms as ordinary lexical closures.
    /// Passing their values instead would sever captured mutable cells.
    fn specialized_reference(
        &self,
        original: &str,
        specialized: &str,
        regions: &[Region],
    ) -> Option<Expr> {
        let Stmt::Function { parameters, .. } = self.definitions.get(original)? else {
            return None;
        };
        let captures: Vec<Expr> = regions
            .iter()
            .flat_map(|region| region.arms.iter())
            .filter(|arm| captures_lexical_values(arm))
            .map(|arm| Expr::Lambda {
                parameters: arm.params.iter().map(|name| parameter(name)).collect(),
                return_type: None,
                body: Box::new(arm.body.clone()),
                position: arm.position,
            })
            .collect();
        if captures.is_empty() {
            return Some(Expr::Identifier(specialized.to_owned()));
        }
        let mut arguments: Vec<Expr> = parameters
            .iter()
            .map(|param| Expr::Identifier(param.name.clone()))
            .collect();
        arguments.extend(captures);
        Some(Expr::Lambda {
            parameters: parameters.clone(),
            return_type: None,
            body: Box::new(call(specialized, arguments)),
            position: None,
        })
    }
}

fn captures_lexical_values(arm: &HandlerArm) -> bool {
    let mut names = BTreeSet::new();
    crate::freevars::free_idents(&arm.body, &mut names);
    names
        .iter()
        .any(|name| name.contains("$stage") && !arm.params.contains(name))
}

fn callback_name(region: u32, operation: &str) -> String {
    format!("$stage_arm{region}_{operation}")
}

fn parameter(name: &str) -> Parameter {
    Parameter {
        name: name.to_owned(),
        ty: None,
        inline_constraint: false,
    }
}

fn call(name: &str, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function: Box::new(Expr::Identifier(name.to_owned())),
        arguments,
        named_arguments: Vec::new(),
    }
}

/// Whether any enclosing region answers `effect`.
fn answered(regions: &[Region], effect: &str) -> bool {
    regions.iter().any(|region| region.effect == effect)
}

/// The innermost arm answering one operation.
fn innermost_arm(regions: &[Region], effect: &str, operation: &str) -> Option<(usize, HandlerArm)> {
    regions
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, region)| region.effect == effect)
        .find_map(|(index, region)| {
            region
                .arms
                .iter()
                .find(|arm| arm.operation == operation)
                .map(|arm| (index, arm.clone()))
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
