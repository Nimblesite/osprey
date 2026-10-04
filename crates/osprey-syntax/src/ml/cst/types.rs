//! ML concrete types.
use super::{Multiplicity, OperationMode, Position};

/// Declaration-site variance of a type parameter, exactly as written
/// ([TYPE-VARIANCE-DECL]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MlVariance {
    /// Unannotated.
    Invariant,
    /// `out T`.
    Covariant,
    /// `in T`.
    Contravariant,
}

/// One declared type parameter with its optional variance marker.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlTypeParam {
    /// The parameter name.
    pub name: String,
    /// The written variance marker (`Invariant` when unannotated).
    pub variance: MlVariance,
}

/// One effect reference inside an effect row, optionally applied to type
/// arguments (`State<int>`) ([EFFECTS-GENERIC-ROWS]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlEffectRef {
    /// The effect name.
    pub name: String,
    /// The applied type arguments (empty for a bare reference).
    pub args: Vec<MlType>,
    /// Source position of the effect name.
    pub pos: Position,
}

/// A parenthesised `name : type` parameter of an `extern` declaration.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlExternParam {
    /// The parameter name.
    pub name: String,
    /// The parameter's declared type.
    pub ty: MlType,
}

/// One `op : P => R` operation line of an `effect` declaration. The payload and
/// result types are rendered into the canonical `fn(P) -> R` string by the
/// lowerer ([FLAVOR-ML-EFFECT]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlEffectOp {
    /// The operation name.
    pub name: String,
    /// Whether `control` was written ([EFFECTS-HANDLER-ARMS]).
    pub mode: OperationMode,
    /// The multiplicity keyword as written, absent when undecorated
    /// ([MULTI-DECL]).
    pub multiplicity: Option<Multiplicity>,
    /// Whether re-performing the operation is acceptable ([MULTI-REPLAY]).
    pub replayable: bool,
    /// The operation's payload (argument) type.
    pub payload: MlType,
    /// The operation's result type.
    pub result: MlType,
    /// Raw `(** … *)` text preceding this operation line, still unparsed —
    /// `lower_effect_op` runs it through the shared flavor-neutral doc parser
    /// so both surfaces yield the same `DocComment` ([DOC-EFFECT-OP]).
    pub doc: Option<String>,
    /// Source position of the operation name.
    pub pos: Position,
}

/// One variant of a `type` declaration: a constructor name and its payload
/// fields (empty for a bare enum case like `Active`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlVariant {
    /// The constructor name.
    pub name: String,
    /// The payload fields, in declaration order.
    pub fields: Vec<MlTypeField>,
}

/// A `field : type` line inside a variant's payload block.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MlTypeField {
    /// The field name.
    pub name: String,
    /// The field's declared type.
    pub ty: MlType,
}

/// An ML type expression. Arrows are right-associative; application binds
/// tighter (`Handler Db`, `Result int string`) ([FLAVOR-ML-FN]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MlType {
    /// A bare type name (`int`, `string`, `Unit`, a user type).
    Name(String),
    /// Type application `head arg…` (`Handler Db`, `Result int string`).
    App {
        /// The head type name.
        head: String,
        /// The applied argument types.
        args: Vec<MlType>,
    },
    /// `a -> b` (right-associative).
    Arrow {
        /// The argument type.
        from: Box<MlType>,
        /// The result type.
        to: Box<MlType>,
    },
    /// `(a, b, …)` a tupled single argument.
    Tuple(Vec<MlType>),
}
