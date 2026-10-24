//! Specialization of lexically resolved immutable callable bindings.

use super::{Lowering, Region};
use crate::{Expr, Parameter, Position, Program, Stmt};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn collect(program: &Program, definitions: &BTreeMap<String, Stmt>) -> BTreeMap<String, Expr> {
    #[derive(Default)]
    struct Bindings(BTreeMap<String, Expr>);
    impl crate::AstVisitor for Bindings {
        fn statement(&mut self, statement: &Stmt) {
            match statement {
                Stmt::Let {
                    name,
                    mutable: false,
                    value,
                    ..
                } if matches!(value, Expr::Lambda { .. } | Expr::Identifier(_)) => {
                    let _ = self.0.insert(name.clone(), value.clone());
                }
                // A function whose body is a static region is a handler
                // value a module can export, since a module-level `let` must
                // be a constant; it applies like the `let` form. Parameters
                // beyond the callback are values its arms capture, and the
                // region follows each instantiation's callback (#274).
                Stmt::Function {
                    name,
                    parameters,
                    body: body @ Expr::Handler { stage, .. },
                    ..
                } if stage.is_compile_time() => {
                    let _ = self.0.insert(
                        name.clone(),
                        Expr::Lambda {
                            parameters: parameters.clone(),
                            return_type: None,
                            body: Box::new(body.clone()),
                            position: None,
                        },
                    );
                }
                _ => {}
            }
        }
    }
    let mut bindings = Bindings::default();
    crate::walk_program(program, &mut bindings);
    bindings.0.extend(super::factories::collect(program, definitions));
    bindings.0
}

/// The static region `function` names through `let` aliases: the aliases
/// passed through, its parameters and its region.
pub(super) fn installer<'a>(
    bindings: &'a BTreeMap<String, Expr>,
    function: &'a Expr,
) -> Option<(Vec<String>, &'a [Parameter], &'a Expr)> {
    let mut selected = function;
    let mut aliases = Vec::new();
    while let Expr::Identifier(name) = selected {
        if aliases.contains(name) {
            return None;
        }
        selected = bindings.get(name)?;
        aliases.push(name.clone());
    }
    let Expr::Lambda {
        parameters, body, ..
    } = selected
    else {
        return None;
    };
    matches!(body.as_ref(), Expr::Handler { stage, .. } if stage.is_compile_time()).then_some((
        aliases,
        parameters,
        body.as_ref(),
    ))
}

impl Lowering {
    /// A callable static interpretation must be selected before its callback is
    /// specialized. Erasing the region at definition time loses the callback's
    /// operations, so the region applies at each instantiation instead: every
    /// resolvable callback argument is substituted and the remaining arguments
    /// are bound once, in order, discharging what the callback requires
    /// ([STAGE-POLY], [STAGE-POLY-ERASURE]). Arbitrary runtime-selected
    /// handlers still require a dynamic region.
    pub(super) fn apply_static_handler(&mut self, expression: &mut Expr) -> bool {
        let Some(installation) = self.installation(expression) else {
            return false;
        };
        let Some((effect, position)) = installation.handled() else {
            return false;
        };
        if !self.spend_fuel(effect, "handler application", position) {
            return false;
        }
        let Expr::Call { arguments, .. } = &*expression else {
            return false;
        };
        let applied = self.inline(&installation.parameters, &installation.region, arguments);
        self.consumed.extend(installation.aliases);
        *expression = applied;
        true
    }

    /// The installer application `expression` makes, when it is a plain
    /// positional call whose static region a resolvable callback argument
    /// reaches; only such an application can discharge.
    fn installation(&self, expression: &Expr) -> Option<Installation> {
        let (function, arguments) = positional_call(expression)?;
        let (aliases, parameters, region) = installer(&self.bindings, function)?;
        let applicable = parameters.len() == arguments.len()
            && self.callback_reaches_region(parameters, arguments, region);
        applicable.then(|| Installation {
            aliases,
            parameters: parameters.to_vec(),
            region: region.clone(),
        })
    }

    fn callback_reaches_region(
        &self,
        parameters: &[Parameter],
        arguments: &[Expr],
        region: &Expr,
    ) -> bool {
        parameters
            .iter()
            .zip(arguments)
            .any(|(parameter, argument)| {
                self.is_callback(argument) && mentions(region, &parameter.name)
            })
    }

    pub(super) fn specialize_binding(&mut self, name: &str, regions: &[Region]) -> Option<Expr> {
        let mut value = self.bindings.get(name)?.clone();
        // Keep closures in their lexical scope: their captures remain binding
        // references, including shared mutable cells, rather than copied values.
        let mut visible: Vec<Region> = regions.iter().map(Region::clone_region).collect();
        self.rewrite(&mut value, &mut visible);
        let _ = self.consumed.insert(name.to_owned());
        Some(value)
    }
}

/// One installer application selected for rewriting: the `let` aliases it
/// passed through, the installer's parameters and its static region.
struct Installation {
    aliases: Vec<String>,
    parameters: Vec<Parameter>,
    region: Expr,
}

impl Installation {
    /// The handled effect and the region's position, for the rewrite ledger.
    fn handled(&self) -> Option<(&str, Option<Position>)> {
        match &self.region {
            Expr::Handler {
                effect, position, ..
            } => Some((effect, *position)),
            _ => None,
        }
    }
}

/// A call without named arguments: its callee and positional arguments. Named
/// arguments never select an installer.
fn positional_call(expression: &Expr) -> Option<(&Expr, &[Expr])> {
    match expression {
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } if named_arguments.is_empty() => Some((function, arguments)),
        _ => None,
    }
}

pub(super) fn substitute(expression: &mut Expr, parameter: &str, action: &Expr) {
    if matches!(expression, Expr::Identifier(name) if name == parameter) {
        *expression = action.clone();
    } else {
        crate::mutate::children_mut(expression, &mut |child| {
            substitute(child, parameter, action);
        });
    }
}

/// Whether `region`'s body names `parameter`: only a parameter the region
/// reaches can carry its arguments' operations into it.
fn mentions(region: &Expr, parameter: &str) -> bool {
    let mut free = BTreeSet::new();
    crate::freevars::free_idents(region, &mut free);
    free.contains(parameter)
}
