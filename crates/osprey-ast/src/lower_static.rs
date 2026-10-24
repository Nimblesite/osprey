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
use crate::stage_rows::operation_name;
use crate::{EffectRef, Expr, HandlerArm, Parameter, Position, Program, Stmt, TypeExpr};
use std::collections::{BTreeMap, BTreeSet};

/// The program entry, which nothing references and pruning must never remove.
const ENTRY_POINT: &str = "main";

mod bindings;
mod factories;
mod forwarding;
mod handlers;
mod hygiene;
mod region;
mod requirements;
use region::{answered, innermost_arm, specialization_key, Region};
pub(crate) use requirements::function_definitions;
use requirements::{all_requirements, retain_referenced_originals, static_performers};

/// The pass state: the program's function definitions plus what the rewrite has
/// produced so far.
struct Lowering {
    /// Every named function's definition, keyed by name.
    definitions: BTreeMap<String, Stmt>,
    bindings: BTreeMap<String, Expr>,
    forwarders: BTreeSet<String>,
    effects: BTreeMap<String, EffectDecl>,
    /// Each function's requirements, independent of its selected interpretation.
    requirements: BTreeMap<String, Vec<String>>,
    dispatches: BTreeMap<String, Vec<String>>,
    /// What each call site settles about the helper it calls.
    callables: crate::stage_rows::Callables,
    /// Region-specialized definitions appended to the program.
    produced: Vec<Stmt>,
    /// Specialization cache, keyed by function name and the operations each
    /// enclosing region still answers there.
    emitted: BTreeMap<String, String>,
    /// Names whose original definition the regions consumed.
    consumed: BTreeSet<String>,
    errors: Vec<StageError>,
    regions: u32,
    fuel: u32,
    /// How many substitutions are currently nested, so a diverging rewrite is
    /// reported rather than overflowing the stack ([`REWRITE_DEPTH_BOUND`]).
    depth: u32,
    /// Closures the rewrite wraps around a specialization have no source
    /// position, but inference addresses a lambda by its position. Each gets
    /// its own synthetic one, on line 0, which no source line occupies.
    wrappers: u32,
    /// Effects whose enclosing static regions a runtime handler currently
    /// shadows: a `mask` of one would skip into a discharged region.
    shadowed: Vec<String>,
}

/// Discharge every static handler in `program`.
pub(crate) fn run(program: &Program) -> Result<Program, Vec<StageError>> {
    if !has_regions(program) {
        return Ok(program.clone());
    }
    let program = hygiene::resolve(program);
    let definitions = function_definitions(&program);
    let bindings = bindings::collect(&program, &definitions);
    let requirements = all_requirements(&program, &bindings);
    let mut lowering = Lowering {
        forwarders: forwarding::collect(&definitions, &bindings),
        definitions,
        bindings,
        effects: effect_declarations(&program),
        requirements,
        dispatches: crate::stage_rows::dispatch_requirements(&program),
        callables: crate::stage_rows::Callables::of(&program),
        produced: Vec::new(),
        emitted: BTreeMap::new(),
        consumed: BTreeSet::new(),
        errors: Vec::new(),
        regions: 0,
        fuel: REWRITE_BOUND,
        depth: 0,
        wrappers: 0,
        shadowed: Vec::new(),
    };
    let mut rewritten = program;
    for statement in &mut rewritten.statements {
        statement_children_mut(statement, &mut |expression| {
            lowering.rewrite(expression, &mut Vec::new());
        });
    }
    rewritten.statements.extend(lowering.produced);
    let mut prunable = static_performers(&rewritten, &lowering.effects);
    prunable.extend(lowering.consumed);
    // Broken-code quarantine: pruning a `main` whose request the rewrite could
    // not reach compiled the program to nothing; the residual row names it.
    let _ = prunable.remove(ENTRY_POINT);
    retain_referenced_originals(&mut rewritten, &prunable);
    if lowering.errors.is_empty() {
        Ok(rewritten)
    } else {
        Err(lowering.errors)
    }
}

/// Give every local binding a name unique in the program, so a term moved
/// between scopes can neither capture nor be captured.
pub(crate) fn resolve_bindings(program: &Program) -> Program {
    hygiene::resolve(program)
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
        if self.apply_static_handler(expression) || self.apply_forwarder(expression) {
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
            Expr::Handler { .. } => self.enter_runtime_handler(expression, regions),
            Expr::Mask { .. } => self.enter_mask(expression, regions),
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
            let declared = self
                .effects
                .get(crate::effect_name::base(&effect))
                .map_or(&[][..], |decl| decl.parameter_types(&operation));
            *expression = bind_parameters(&arm, &arguments, declared);
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
        if !self.reaches_answered_operation(name, regions) {
            return None;
        }
        if let Some(value) = self.specialize_binding(name, regions) {
            return Some(value);
        }
        let key = specialization_key(name, regions);
        if let Some(existing) = self.emitted.get(&key).cloned() {
            return self.specialized_reference(name, &existing, regions);
        }
        let specialized = format!("{name}__stage{}", self.emitted.len().saturating_add(1));
        let _ = self.emitted.insert(key, specialized.clone());
        self.produce(name, &specialized, regions);
        self.specialized_reference(name, &specialized, regions)
    }

    /// Whether `name` is a function requiring an operation these regions answer.
    fn reaches_answered_operation(&self, name: &str, regions: &[Region]) -> bool {
        self.requirements.get(name).is_some_and(|operations| {
            operations
                .iter()
                .any(|operation| regions.iter().any(|region| region.answers(operation)))
        })
    }

    /// The declared effects a specialization still carries. Regions answering
    /// every operation of an effect discharge its declaration; a shadowed
    /// region answering only some leaves it in place. Implements [STAGE-RESIDUE].
    fn residual_declarations(
        &self,
        declared: Vec<EffectRef>,
        regions: &[Region],
    ) -> Vec<EffectRef> {
        declared
            .into_iter()
            .filter(|reference| {
                let effect =
                    crate::effect_name::instantiated(&reference.name, &reference.type_args);
                !self.answers_every_operation(regions, &effect)
            })
            .collect()
    }

    fn answers_every_operation(&self, regions: &[Region], effect: &str) -> bool {
        self.effects
            .get(crate::effect_name::base(effect))
            .is_some_and(|declared| {
                declared.operations.iter().all(|operation| {
                    let name = operation_name(effect, &operation.name);
                    regions.iter().any(|region| region.answers(&name))
                })
            })
    }

    /// Emit one region-owned copy of a function, rewritten under the regions.
    fn produce(&mut self, original: &str, specialized: &str, regions: &[Region]) {
        let Some(Stmt::Function {
            type_params,
            mut parameters,
            return_type,
            effects,
            effect_tail,
            effect_row_present,
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
        let effects = self.residual_declarations(effects, regions);
        let required = self.remaining_requirements(&specialized_body);
        let dispatches = self.dispatches_of(&specialized_body);
        let _ = self.dispatches.insert(specialized.to_owned(), dispatches);
        let _ = self.requirements.insert(specialized.to_owned(), required);
        let _ = self.consumed.insert(original.to_owned());
        self.produced.push(Stmt::Function {
            name: specialized.to_owned(),
            type_params,
            parameters,
            return_type,
            effects,
            effect_tail,
            effect_row_present,
            body: specialized_body,
            doc,
            position,
        });
    }

    /// A specialization receives capturing arms as ordinary lexical closures.
    /// Passing their values instead would sever captured mutable cells.
    fn specialized_reference(
        &mut self,
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
        self.wrappers = self.wrappers.saturating_add(1);
        Some(Expr::Lambda {
            parameters: parameters.clone(),
            return_type: None,
            body: Box::new(call(specialized, arguments)),
            position: Some(Position {
                line: 0,
                column: self.wrappers,
            }),
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

/// The arm body with the operation's arguments bound to the arm's parameters,
/// each under the type the operation declared for it where that type is closed
/// — so the residual keeps the contract the request was checked against. A
/// nullary operation needs no block, which keeps the common reactive read
/// ([STAGE-SIGNALS]) a bare expression. Implements [STAGE-BACKEND-CONTRACT].
fn bind_parameters(arm: &HandlerArm, arguments: &[Expr], declared: &[Option<TypeExpr>]) -> Expr {
    if arm.params.is_empty() {
        return arm.body.clone();
    }
    Expr::Block {
        statements: arm
            .params
            .iter()
            .zip(arguments)
            .enumerate()
            .map(|(index, (name, value))| Stmt::Let {
                name: name.clone(),
                mutable: false,
                ty: declared.get(index).cloned().flatten(),
                value: value.clone(),
                doc: None,
                position: None,
            })
            .collect(),
        value: Some(Box::new(arm.body.clone())),
        position: None,
    }
}
