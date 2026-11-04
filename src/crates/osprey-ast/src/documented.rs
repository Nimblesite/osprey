//! Which declarations carry their own documentation ([DOC-ATTACH]), shared by
//! the documentation exporter and the undocumented-export warning.

use crate::{DocComment, Position, Stmt};

/// A documentable declaration: its name, kind, outer doc and position.
#[derive(Debug, Clone, Copy)]
pub struct Documented<'a> {
    /// The declared name.
    pub name: &'a str,
    /// `"Function"`, `"Value"`, `"Extern"`, `"Type"`, `"Effect"` or `"Signature"`.
    pub kind: &'static str,
    /// The outer documentation comment, when written.
    pub doc: Option<&'a DocComment>,
    /// Where the declaration was written, when recorded.
    pub position: Option<Position>,
}

impl Stmt {
    /// This statement as a documentable declaration. Modules and namespaces
    /// are scopes with inner docs too, so they are not answered here.
    #[must_use]
    pub fn documented(&self) -> Option<Documented<'_>> {
        let (name, kind, doc, position) = match self {
            Self::Function {
                name,
                doc,
                position,
                ..
            } => (name, "Function", doc, position),
            Self::Let {
                name,
                doc,
                position,
                ..
            } => (name, "Value", doc, position),
            Self::Extern {
                name,
                doc,
                position,
                ..
            } => (name, "Extern", doc, position),
            Self::Type {
                name,
                doc,
                position,
                ..
            } => (name, "Type", doc, position),
            Self::Effect {
                name,
                doc,
                position,
                ..
            } => (name, "Effect", doc, position),
            Self::Signature {
                name,
                doc,
                position,
                ..
            } => (name, "Signature", doc, position),
            _ => return None,
        };
        Some(Documented {
            name,
            kind,
            doc: doc.as_ref(),
            position: *position,
        })
    }
}
