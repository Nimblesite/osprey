//! Specialization of lexically resolved immutable callable bindings.

use super::{Lowering, Region};
use crate::{Expr, Program, Stmt};
use std::collections::BTreeMap;

pub(super) fn collect(program: &Program) -> BTreeMap<String, Expr> {
    #[derive(Default)]
    struct Bindings(BTreeMap<String, Expr>);
    impl crate::AstVisitor for Bindings {
        fn statement(&mut self, statement: &Stmt) {
            if let Stmt::Let {
                name,
                mutable: false,
                value,
                ..
            } = statement
            {
                if matches!(value, Expr::Lambda { .. } | Expr::Identifier(_)) {
                    let _ = self.0.insert(name.clone(), value.clone());
                }
            }
        }
    }
    let mut bindings = Bindings::default();
    crate::walk_program(program, &mut bindings);
    bindings.0
}

impl Lowering {
    /// A callable static interpretation must be selected before its callback is
    /// specialized. Erasing the lambda's region at definition time loses that
    /// callback's operations. Resolve only known closure/function arguments;
    /// arbitrary runtime-selected handlers still require a dynamic region.
    pub(super) fn apply_static_handler(&mut self, expression: &mut Expr) -> bool {
        let Expr::Call {
            function,
            arguments,
            named_arguments,
        } = expression
        else {
            return false;
        };
        let [action] = arguments.as_slice() else {
            return false;
        };
        if !named_arguments.is_empty()
            || !matches!(action, Expr::Lambda { .. } | Expr::Identifier(_))
        {
            return false;
        }
        let mut selected = function.as_ref();
        let mut aliases = Vec::new();
        while let Expr::Identifier(name) = selected {
            if aliases.contains(name) {
                return false;
            }
            let Some(value) = self.bindings.get(name) else {
                return false;
            };
            aliases.push(name.clone());
            selected = value;
        }
        let Expr::Lambda {
            parameters, body, ..
        } = selected
        else {
            return false;
        };
        let [parameter] = parameters.as_slice() else {
            return false;
        };
        let Expr::Handler {
            stage,
            effect,
            position,
            ..
        } = body.as_ref()
        else {
            return false;
        };
        if !stage.is_compile_time() {
            return false;
        }
        let mut applied = body.as_ref().clone();
        let parameter = parameter.name.clone();
        let effect = effect.clone();
        let position = *position;
        let action = action.clone();
        if !self.spend_fuel(&effect, "handler application", position) {
            return false;
        }
        substitute(&mut applied, &parameter, &action);
        self.consumed.extend(aliases);
        *expression = applied;
        true
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

fn substitute(expression: &mut Expr, parameter: &str, action: &Expr) {
    if matches!(expression, Expr::Identifier(name) if name == parameter) {
        *expression = action.clone();
    } else {
        crate::mutate::children_mut(expression, &mut |child| {
            substitute(child, parameter, action);
        });
    }
}
