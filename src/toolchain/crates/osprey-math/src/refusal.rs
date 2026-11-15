//! The closed refusal vocabulary of [MATH-CODE-REFUSALS].

/// Why a math source generates nothing. The set is closed: a new code is a
/// specification change with its own test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// A Lean command outside the fragment.
    UnsupportedCommand,
    /// A Lean term form outside the fragment.
    UnsupportedTerm,
    /// A type other than `Float` or `Bool`, or a missing one.
    UnsupportedType,
    /// A Lean function with no equal Osprey operation.
    UnsupportedFunction,
    /// A numeric literal form the fragment does not read.
    UnsupportedLiteral,
    /// A literal that is not finite in binary64.
    LiteralOutOfRange,
    /// A name with no definition in scope.
    UnknownName,
    /// A name Osprey cannot spell.
    UnsupportedName,
    /// A name that would capture an Osprey keyword or builtin.
    ReservedName,
    /// A `let` reusing a name already in scope.
    ShadowedName,
    /// Two definitions with one name.
    DuplicateName,
    /// A call with the wrong number of arguments.
    ArityMismatch,
    /// An operand, argument, branch or body of the wrong type.
    TypeMismatch,
    /// An `if` or `let` where only an operand may stand.
    NestedConditional,
    /// An argument on a later line than its function.
    MultilineApplication,
    /// A namespace the generated module cannot mirror.
    NamespaceShape,
    /// A token the fragment's grammar does not allow here.
    UnexpectedToken,
}

/// One refusal: where the source stops being readable and the shortest change
/// that makes it readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Which rule refused.
    pub code: Code,
    /// One-based source line.
    pub line: usize,
    /// Zero-based column, in Unicode scalar values.
    pub column: usize,
    /// What was found and the fix.
    pub message: String,
}

impl Refusal {
    pub(crate) fn at(code: Code, place: (usize, usize), message: impl Into<String>) -> Self {
        Self {
            code,
            line: place.0,
            column: place.1,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            code,
            line,
            column,
            message,
        } = self;
        write!(f, "{line}:{column}: {code:?}: {message}")
    }
}
