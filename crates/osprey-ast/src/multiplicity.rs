//! Resumption multiplicity — how many times an operation's request may be
//! answered.
//!
//! Stage says *when* a request is answered ([`Stage`](crate::Stage));
//! multiplicity says *how many times*. A handler that resumes twice re-runs the
//! remainder of the handled computation, so a body that sends an email sends it
//! twice; a handler that never resumes ends the computation where it stands.
//! Those are different programs, they cost different amounts to represent, and
//! they run on different targets. Implements [MULTI-AXIS].

use crate::{Program, Stage, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// How many times a handler may answer one request. Implements [MULTI-AXIS].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Multiplicity {
    /// Never resumes — the arm's value answers the whole `handle` region and
    /// the `perform` never returns. Implements [MULTI-HANDLE-ABORT].
    Abort,
    /// Resumes at most once. The default, what the runtime already enforces,
    /// and the only shape WebAssembly will support. Affine, not linear:
    /// dropping a continuation is always the safe direction.
    /// Implements [MULTI-HANDLE-ONCE].
    #[default]
    Once,
    /// Resumes any number of times — backtracking, search, nondeterminism.
    /// Implements [MULTI-HANDLE-MANY].
    Many,
}

/// The marker asserting an operation is safe to re-perform. Spelled once here
/// so both flavors' lowerers and every diagnostic agree. Implements
/// [MULTI-REPLAY].
pub const REPLAYABLE_KEYWORD: &str = "replayable";

impl Multiplicity {
    /// The keyword that declares this multiplicity, as diagnostics spell it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::Once => "once",
            Self::Many => "many",
        }
    }

    /// Parse a declaration keyword. `None` for anything else, so a lowerer can
    /// treat an absent or unrecognised marker as the [`Multiplicity::Once`]
    /// default without a second table.
    #[must_use]
    pub fn from_keyword(word: &str) -> Option<Self> {
        match word {
            "abort" => Some(Self::Abort),
            "once" => Some(Self::Once),
            "many" => Some(Self::Many),
            _ => None,
        }
    }
}

/// Every declared operation's multiplicity and replayability, keyed by
/// `(effect, operation)`.
///
/// One index, read by everything that needs the axis: the checker's handle-site
/// rules and the per-target capability gate both decide from the DECLARATION,
/// so both must read the same table or they can disagree about the same
/// program. Implements [MULTI-AXIS].
#[derive(Debug, Default)]
pub struct OperationTable {
    declared: BTreeMap<(String, String), Declared>,
    /// Operations of a `static` effect. They are rewritten away before the
    /// checker runs, so re-running them is re-running ordinary code — the gate
    /// [GPU-KERNEL-PURE] already assumes is harmless. Implements
    /// [MULTI-REPLAY-CHECK].
    static_effects: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy)]
struct Declared {
    multiplicity: Multiplicity,
    replayable: bool,
}

impl OperationTable {
    /// Index every effect declaration in `program`, at any nesting depth.
    #[must_use]
    pub fn collect(program: &Program) -> Self {
        let mut table = Self::default();
        crate::walk_program(program, &mut table);
        table
    }

    /// How many times `effect.operation` may be answered. An operation the
    /// program never declared reads as the [`Multiplicity::Once`] default, so a
    /// missing declaration is diagnosed by the checker that owns that error
    /// rather than twice. Implements [MULTI-COMPAT].
    #[must_use]
    pub fn multiplicity_of(&self, effect: &str, operation: &str) -> Multiplicity {
        self.declared
            .get(&(effect.to_string(), operation.to_string()))
            .map_or_else(Multiplicity::default, |d| d.multiplicity)
    }

    /// Whether re-performing `effect.operation` is acceptable to the program.
    /// Implements [MULTI-REPLAY].
    #[must_use]
    pub fn is_replayable(&self, effect: &str, operation: &str) -> bool {
        self.static_effects.contains(effect)
            || self
                .declared
                .get(&(effect.to_string(), operation.to_string()))
                .is_some_and(|d| d.replayable)
    }
}

impl crate::AstVisitor for OperationTable {
    fn statement(&mut self, statement: &Stmt) {
        let Stmt::Effect {
            stage,
            name,
            operations,
            ..
        } = statement
        else {
            return;
        };
        if *stage == Stage::Static {
            let _ = self.static_effects.insert(name.clone());
        }
        for operation in operations {
            let _ = self.declared.insert(
                (name.clone(), operation.name.clone()),
                Declared {
                    multiplicity: operation.multiplicity(),
                    replayable: operation.replayable,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EffectOperation;

    #[test]
    fn keywords_round_trip_and_default_is_once() {
        assert_eq!(Multiplicity::default(), Multiplicity::Once);
        for m in [Multiplicity::Abort, Multiplicity::Once, Multiplicity::Many] {
            assert_eq!(Multiplicity::from_keyword(m.as_str()), Some(m));
        }
        assert_eq!(Multiplicity::from_keyword(REPLAYABLE_KEYWORD), None);
        assert_eq!(Multiplicity::from_keyword("twice"), None);
    }

    #[test]
    fn the_table_reads_declarations_and_defaults_the_rest() {
        let operation = |name: &str, multiplicity, replayable| EffectOperation {
            name: name.into(),
            declared_multiplicity: multiplicity,
            replayable,
            ty: "fn() -> int".into(),
            parameters: Vec::new(),
            return_type: String::new(),
            doc: None,
            position: None,
        };
        let effect = |name: &str, stage, operations| Stmt::Effect {
            stage,
            name: name.into(),
            type_params: Vec::new(),
            operations,
            doc: None,
            position: None,
        };
        let program = Program {
            statements: vec![
                effect(
                    "Choice",
                    crate::Stage::Dynamic,
                    vec![
                        operation("pick", Some(Multiplicity::Many), false),
                        operation("seed", None, true),
                    ],
                ),
                effect(
                    "Tile",
                    crate::Stage::Static,
                    vec![operation("size", None, false)],
                ),
            ],
        };
        let table = OperationTable::collect(&program);
        assert_eq!(table.multiplicity_of("Choice", "pick"), Multiplicity::Many);
        // Undecorated and undeclared alike read as the `once` default.
        assert_eq!(table.multiplicity_of("Choice", "seed"), Multiplicity::Once);
        assert_eq!(table.multiplicity_of("Absent", "gone"), Multiplicity::Once);
        assert!(table.is_replayable("Choice", "seed"));
        assert!(!table.is_replayable("Choice", "pick"));
        // Static entries count as replayable: after discharge they are ordinary
        // code, and re-running ordinary code is already assumed harmless.
        assert!(table.is_replayable("Tile", "size"));
        assert!(!table.is_replayable("Absent", "gone"));
    }
}
