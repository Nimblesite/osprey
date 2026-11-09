//! Match and select patterns.

use crate::Expr;
use crate::TypeExpr;
/// A pattern in a match/select arm.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `_` — matches anything, binds nothing.
    Wildcard,
    /// A literal pattern (int/float/string/bool).
    Literal(Box<Expr>),
    /// `Ctor` / `Ctor { a, b }` / `Ctor(p, ...)` constructor & destructuring forms.
    Constructor {
        /// Constructor name.
        name: String,
        /// Bound field names (`{ a, b }` form).
        fields: Vec<String>,
        /// Positional sub-patterns (`Ctor(p, ...)`), or binders aligned with
        /// `fields` for explicitly named aliases (`Ctor(field=binder)`).
        sub_patterns: Vec<Pattern>,
    },
    /// `value: Int` type-annotated binding.
    TypeAnnotated {
        /// Bound name.
        name: String,
        /// The annotated type.
        ty: TypeExpr,
    },
    /// `{ name, age }` / `{ name, .. }` structural row pattern. A tuple pattern
    /// `(a, b)` is the positional spelling: field names are the decimal slots
    /// `"0"`, `"1"`, … ([`positional_field_name`]) with the written binders.
    /// Implements [PATTERN-STRUCTURAL], [PATTERN-TUPLE].
    Structural {
        /// `(field name, binder name)` pairs in written order. A brace pattern
        /// binds each field under its own name; an empty binder (`_` in a tuple
        /// slot) matches the field without binding it.
        fields: Vec<(String, String)>,
        /// Whether a trailing `..` opens the row to any value carrying at
        /// least the named fields; a closed pattern selects the exact row.
        open: bool,
    },
    /// `[]` / `[a, b]` / `[head, ...tail]` list destructuring. `elements` are the
    /// fixed-prefix position patterns; `rest` is the optional tail binder name
    /// (`...rest`), `None` for a fixed-length match. Implements
    /// [TYPE-LIST-PATTERNS].
    List {
        /// Patterns for the fixed-prefix element positions.
        elements: Vec<Pattern>,
        /// Tail rest-binder name, or `None` for a fixed-length pattern.
        rest: Option<String>,
    },
    /// A bare identifier capture.
    Binding(String),
}
