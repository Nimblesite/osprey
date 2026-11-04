//! Code generation errors. The generator never panics — an unsupported
//! construct or a malformed program returns `Err` so the CLI can report it like
//! any other diagnostic (CLAUDE.md: fail loudly, never emit a placeholder).

use std::fmt;

/// A code generation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    /// A language construct the textual-IR backend does not lower yet.
    Unsupported(String),
    /// A reference to a name with no binding in scope.
    UnknownName(String),
    /// A structurally invalid program (e.g. a call with no callee).
    Invalid(String),
}

impl CodegenError {
    /// An unsupported construct the backend does not lower yet.
    pub(crate) fn unsupported(what: impl Into<String>) -> CodegenError {
        CodegenError::Unsupported(what.into())
    }
    /// A reference to an unknown name (function, constructor, variable).
    pub(crate) fn unknown(name: impl Into<String>) -> CodegenError {
        CodegenError::UnknownName(name.into())
    }
    /// A program that is structurally invalid for codegen.
    pub(crate) fn invalid(why: impl Into<String>) -> CodegenError {
        CodegenError::Invalid(why.into())
    }
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodegenError::Unsupported(w) => write!(f, "codegen: unsupported construct: {w}"),
            CodegenError::UnknownName(n) => write!(f, "codegen: unknown name `{n}`"),
            CodegenError::Invalid(w) => write!(f, "codegen: invalid program: {w}"),
        }
    }
}

impl std::error::Error for CodegenError {}

/// Convenience alias for code generation results.
pub type Result<T> = std::result::Result<T, CodegenError>;
