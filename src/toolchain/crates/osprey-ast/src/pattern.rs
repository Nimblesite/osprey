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
        /// `(field name, slot pattern)` pairs in written order. A bare field
        /// binds under its own name (a [`Pattern::Binding`]); `_` matches the
        /// field without binding it; any other pattern tests the field's value
        /// as its own `match` would ([PATTERN-NESTED]).
        fields: Vec<(String, Pattern)>,
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

impl Pattern {
    /// The name a slot binds directly: a binding's name, or `""` for `_` or a
    /// pattern that tests its value ([PATTERN-NESTED]). A capitalised bare
    /// name is a payload-free variant, so it tests rather than binds.
    #[must_use]
    pub fn slot_binder(&self) -> &str {
        match self {
            Self::Binding(name) if !names_variant(name) => name,
            Self::TypeAnnotated { name, .. } => name,
            _ => "",
        }
    }

    /// Whether the pattern matches every value: a binding or `_`.
    #[must_use]
    pub fn is_irrefutable(&self) -> bool {
        matches!(self, Self::Wildcard) || !self.slot_binder().is_empty()
    }
}

fn names_variant(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// A row's `(field, binder)` pairs, each slot's direct binder or `""`.
#[must_use]
pub fn row_binders(fields: &[(String, Pattern)]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|(field, slot)| (field.clone(), slot.slot_binder().to_owned()))
        .collect()
}
