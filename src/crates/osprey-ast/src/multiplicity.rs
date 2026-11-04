//! The declaration axes of an effect operation: its MODE — whether answering
//! it supplies a value or takes the continuation — and, for a control
//! operation, its resumption multiplicity.
//!
//! Stage says *when* a request is answered ([`Stage`](crate::Stage));
//! multiplicity says *how many times*. A handler that resumes twice re-runs the
//! remainder of the handled computation, so a body that sends an email sends it
//! twice; a handler that never resumes ends the computation where it stands.
//! Those are different programs, they cost different amounts to represent, and
//! they run on different targets. Implements [MULTI-AXIS].

use crate::{Program, Stmt};
use std::collections::BTreeMap;

/// Whether an operation's arm supplies the operation's RESULT or receives the
/// performer's continuation. Declared on the operation, never inferred from an
/// arm's body: the mode is part of the operation's interface, so substituting
/// one handler for another cannot change it, and unreachable code cannot
/// silently reclassify an arm. Implements [EFFECTS-HANDLER-ARMS].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OperationMode {
    /// Undecorated. The arm returns the operation's result `R`; the performer
    /// continues exactly once from where it stood.
    #[default]
    Value,
    /// Declared `control`. The arm receives the suspended remainder and returns
    /// the handler's answer `B`; `resume` is legal only here.
    Control,
}

/// The modifier that declares a control operation. Spelled once so both
/// flavors' lowerers and every diagnostic agree. Implements [MULTI-DECL].
pub const CONTROL_KEYWORD: &str = "control";

impl OperationMode {
    /// True for a `control` operation — the only mode whose arm owns a
    /// continuation.
    #[must_use]
    pub const fn is_control(self) -> bool {
        matches!(self, Self::Control)
    }
}

/// How many times a handler may answer one request. Implements [MULTI-AXIS].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Multiplicity {
    /// Never resumes — the arm's value answers the whole `handle` region and
    /// the `perform` never returns. Implements [MULTI-HANDLE-ABORT].
    Abort,
    /// Resumes at most once. The default. Affine, not linear:
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
}

#[derive(Debug, Clone, Copy)]
struct Declared {
    mode: OperationMode,
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

    /// The declaration a mention names: `Choice<int>.pick` is declared by
    /// `Choice`, so every lookup keys on the base name
    /// [EFFECTS-GENERIC-INSTANTIATION].
    fn declared(&self, effect: &str, operation: &str) -> Option<Declared> {
        self.declared
            .get(&(
                crate::effect_name::base(effect).to_owned(),
                operation.to_owned(),
            ))
            .copied()
    }

    /// Whether `effect.operation` was declared `control`. An operation the
    /// program never declared reads as [`OperationMode::Value`]; the missing
    /// declaration is diagnosed by the checker that owns that error.
    /// Implements [EFFECTS-HANDLER-ARMS].
    #[must_use]
    pub fn mode_of(&self, effect: &str, operation: &str) -> OperationMode {
        self.declared(effect, operation)
            .map_or_else(OperationMode::default, |d| d.mode)
    }

    /// How many times `effect.operation` may be answered. An operation the
    /// program never declared reads as the [`Multiplicity::Once`] default, so a
    /// missing declaration is diagnosed by the checker that owns that error
    /// rather than twice. Implements [MULTI-COMPAT].
    #[must_use]
    pub fn multiplicity_of(&self, effect: &str, operation: &str) -> Multiplicity {
        self.declared(effect, operation)
            .map_or_else(Multiplicity::default, |d| d.multiplicity)
    }

    /// Whether re-performing `effect.operation` is acceptable to the program.
    /// Implements [MULTI-REPLAY].
    #[must_use]
    pub fn is_replayable(&self, effect: &str, operation: &str) -> bool {
        self.declared(effect, operation)
            .is_some_and(|d| d.replayable)
    }
}

impl crate::AstVisitor for OperationTable {
    fn statement(&mut self, statement: &Stmt) {
        let Stmt::Effect {
            name, operations, ..
        } = statement
        else {
            return;
        };
        for operation in operations {
            let _ = self.declared.insert(
                (name.clone(), operation.name.clone()),
                Declared {
                    mode: operation.mode,
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
    use crate::{EffectOperation, Stage};

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
        let operation =
            |name: &str, multiplicity: Option<Multiplicity>, replayable| EffectOperation {
                name: name.into(),
                mode: multiplicity.map_or(OperationMode::Value, |_| OperationMode::Control),
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
                    Stage::Dynamic,
                    vec![
                        operation("pick", Some(Multiplicity::Many), false),
                        operation("seed", None, true),
                    ],
                ),
                effect("Tile", Stage::Static, vec![operation("size", None, false)]),
            ],
            doc: None,
        };
        let table = OperationTable::collect(&program);
        assert_eq!(table.multiplicity_of("Choice", "pick"), Multiplicity::Many);
        assert_eq!(table.mode_of("Choice", "pick"), OperationMode::Control);
        assert_eq!(table.mode_of("Choice", "seed"), OperationMode::Value);
        assert_eq!(table.mode_of("Absent", "gone"), OperationMode::Value);
        // Undecorated and undeclared alike read as the `once` default.
        assert_eq!(table.multiplicity_of("Choice", "seed"), Multiplicity::Once);
        assert_eq!(table.multiplicity_of("Absent", "gone"), Multiplicity::Once);
        assert!(table.is_replayable("Choice", "seed"));
        assert!(!table.is_replayable("Choice", "pick"));
        // A written instantiation names the same declaration
        // [EFFECTS-GENERIC-INSTANTIATION]: `Choice<int>` is declared by `Choice`.
        assert_eq!(
            table.multiplicity_of("Choice<int>", "pick"),
            Multiplicity::Many
        );
        assert_eq!(table.mode_of("Choice<int>", "pick"), OperationMode::Control);
        assert!(table.is_replayable("Choice<List<int>>", "seed"));
        // Replayability is DECLARED. A static operation is not replayable by
        // virtue of being static: its captured state has to earn that
        // ([MULTI-REPLAY-STATE]).
        assert!(!table.is_replayable("Tile", "size"));
        assert!(!table.is_replayable("Absent", "gone"));
    }
}
