//! Effect operations and module signature items.

use crate::DocComment;
use crate::EffectRef;
use crate::Multiplicity;
use crate::OperationMode;
use crate::Parameter;
use crate::Position;
use crate::SignatureAscription;
use crate::Stage;
use crate::SymbolPath;
use crate::TypeExpr;
use crate::TypeParam;
use crate::CONTROL_KEYWORD;
use crate::STATIC_STAGE_KEYWORD;
/// An operation declared inside an `effect` block.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectOperation {
    /// Operation name.
    pub name: String,
    /// Whether the operation was declared `control`. This — not a search of an
    /// arm's body for `resume` — decides whether an arm supplies the
    /// operation's result or the handler's answer. Implements
    /// [EFFECTS-HANDLER-ARMS].
    pub mode: OperationMode,
    /// The multiplicity keyword as WRITTEN, absent when the operation is
    /// undecorated. Kept as an option rather than defaulted at lowering because
    /// [MULTI-AXIS-STATIC] rejects a multiplicity *written* on a static
    /// operation, and `once` — the default — is a legal thing to write.
    /// Read the effective value through [`EffectOperation::multiplicity`].
    /// Implements [MULTI-DECL].
    pub declared_multiplicity: Option<Multiplicity>,
    /// Whether performing this operation twice with the same arguments in the
    /// same handler context is acceptable to the program. Declared, never
    /// inferred: it is a statement about the world outside the program.
    /// Implements [MULTI-REPLAY].
    pub replayable: bool,
    /// The operation's written function type (`fn(T) -> R`).
    pub ty: String,
    /// Parsed parameters of the operation.
    pub parameters: Vec<Parameter>,
    /// The operation's return type as written.
    pub return_type: String,
    /// Structured documentation comment, when written ([DOC-MODEL]). An
    /// operation documents ITSELF; the owning effect's doc describes the
    /// capability as a whole and is only a fallback. Implements
    /// [DOC-EFFECT-OP].
    pub doc: Option<DocComment>,
    /// Source position of the operation name. Implements
    /// [LSP-HOVER-EFFECT-OPERATIONS].
    pub position: Option<Position>,
}

impl EffectOperation {
    /// How many times a handler may answer this operation's request: the
    /// keyword when one was written, otherwise [`Multiplicity::Once`] — which
    /// is what the runtime already enforces, so no existing program changes
    /// meaning. Implements [MULTI-COMPAT].
    #[must_use]
    pub fn multiplicity(&self) -> Multiplicity {
        self.declared_multiplicity.unwrap_or_default()
    }

    /// The defect in this operation's written modifiers under `stage`, if any.
    ///
    /// Multiplicity counts resumptions of a captured continuation, so it says
    /// nothing about a value operation — whose normal return supplies the one
    /// answer. And a static interpretation computes its answer at compile time,
    /// where there is no continuation to hand over, so `control` and `static`
    /// are a contradiction rather than a narrowing.
    /// Implements [MULTI-DECL], [MULTI-AXIS-STATIC].
    #[must_use]
    pub fn modifier_error(&self, effect: &str, stage: Stage) -> Option<String> {
        let name = &self.name;
        if self.mode.is_control() && stage.is_compile_time() {
            return Some(format!(
                "`{CONTROL_KEYWORD}` on operation `{effect}.{name}` of a `{STATIC_STAGE_KEYWORD} effect`; a static interpretation answers at compile time and has no continuation to give away, so neither `{CONTROL_KEYWORD}` nor a multiplicity applies"
            ));
        }
        let declared = self
            .declared_multiplicity
            .filter(|_| !self.mode.is_control())?;
        Some(format!(
            "`{}` on value operation `{name}`; multiplicity counts resumptions, so it is legal only on a `{CONTROL_KEYWORD}` operation",
            declared.as_str()
        ))
    }
}

/// Whether a signature type is abstract or exposes a manifest representation.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureType {
    /// No representation is visible to clients.
    Abstract,
    /// The public representation is the given type expression.
    Manifest(TypeExpr),
}

/// One typed item in an explicit module signature.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureItem {
    /// An immutable exported value (`let empty: Store`).
    Value {
        /// Exported value name.
        name: String,
        /// Declared value type.
        ty: TypeExpr,
        /// Source position of the item.
        position: Option<Position>,
    },
    /// An exported function contract.
    Function {
        /// Exported function name.
        name: String,
        /// Declared generic binders.
        type_params: Vec<TypeParam>,
        /// Parameter types in call order (parameter names are not contractual).
        parameters: Vec<TypeExpr>,
        /// Declared result type.
        return_type: TypeExpr,
        /// Declared effect row.
        effects: Vec<EffectRef>,
        /// Optional open remainder of the effect row (`![Log | e]`).
        effect_tail: Option<String>,
        /// Whether a row was written, including the closed empty row `![]`.
        effect_row_present: bool,
        /// Source position of the item.
        position: Option<Position>,
    },
    /// An abstract or manifest exported type contract.
    Type {
        /// Exported type name.
        name: String,
        /// Declared generic binders.
        type_params: Vec<TypeParam>,
        /// Abstract or manifest representation.
        definition: SignatureType,
        /// Whether the representation stays opaque outside the module even
        /// when the implementation supplies a manifest type.
        opaque: bool,
        /// Source position of the item.
        position: Option<Position>,
    },
    /// An exported algebraic-effect contract.
    Effect {
        /// Exported effect name.
        name: String,
        /// Declared generic binders.
        type_params: Vec<TypeParam>,
        /// Operation contracts.
        operations: Vec<EffectOperation>,
        /// Source position of the item.
        position: Option<Position>,
    },
    /// A nested module constrained by another signature.
    Module {
        /// Nested module path.
        path: SymbolPath,
        /// Required signature and optional-extra policy.
        signature: SignatureAscription,
        /// Source position of the item.
        position: Option<Position>,
    },
}
