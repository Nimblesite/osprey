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
