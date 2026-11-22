//! The built-in constraints a function's signature carries, for readers.

use super::ProgramTypes;
use crate::ty::{Type, VarId};
use std::collections::HashMap;

impl ProgramTypes {
    /// The built-in constraint on each type variable of function `name`'s
    /// signature, read from the obligations its scheme carries: `printable`,
    /// `comparable`, `sized` or `number` [TYPE-CONSTRAINTS].
    #[must_use]
    pub fn constraints(&self, name: &str) -> HashMap<VarId, &'static str> {
        let mut found = HashMap::new();
        let owner = format!("fn {name}");
        for (key, ty) in self.obligations.get(&owner).into_iter().flatten() {
            let Some((constraint, deep)) = crate::builtin_constraints::constraint_of(key) else {
                continue;
            };
            match ty {
                Type::Var(var) => {
                    let _ = found.entry(*var).or_insert(constraint);
                }
                _ if deep => {
                    for var in crate::ty::vars_of(ty) {
                        let _ = found.entry(var).or_insert(constraint);
                    }
                }
                _ => {}
            }
        }
        found
    }
}
