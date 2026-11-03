//! What `structuralEquals` may walk. The built-in compares two values by
//! their contents, so its operand type must be one whose every part has a
//! decided equality — a function, a runtime handle, `any` or `Result` anywhere
//! inside it is a compile error. Implements [BUILTIN-STRUCTURAL-EQUALS]
//! (docs/specs/runtime/0012-Built-InFunctions.md).
//!
//! The requirement is an obligation on the operand type, checked after
//! inference like a comparison's ([`crate::comparison`]), so a generic
//! function carries it to each call site and each is checked at its own type.

use crate::check::Checker;
use crate::convert::type_name_to_type;
use crate::ty::{names, Type};
use std::collections::HashMap;

const STRUCTURAL_EQUALS: &str = "structuralEquals";
const KEY: &str = "structural structuralEquals";

/// One hop on the path from the compared type to an undecided part, so the
/// diagnostic can name the offending field, element or value.
enum Part {
    Field(String),
    Variant(String),
    Element,
    MapValue,
}

/// A named field and its type — one row entry, shared by records and variants.
type Field = (String, Type);

/// A structural record's fields as a row.
fn row_of(fields: &std::collections::BTreeMap<String, Type>) -> Vec<Field> {
    fields
        .iter()
        .map(|(field, ty)| (field.clone(), ty.clone()))
        .collect()
}

impl Checker {
    /// Record the operand type of a `structuralEquals` call as an obligation,
    /// validated after inference so a later-constrained variable is checked at
    /// its final type. Implements [BUILTIN-STRUCTURAL-EQUALS].
    pub(crate) fn constrain_structural_equals(&mut self, name: Option<&str>, args: &[Type]) {
        if let (Some(STRUCTURAL_EQUALS), Some(operand)) = (name, args.first()) {
            let key = self.sited_use(KEY);
            self.builtin_uses.push((key, operand.clone()));
        }
    }

    /// The diagnostic for a `structuralEquals` operand holding a part whose
    /// equality is not decided; `None` for any other obligation, or a decided
    /// one. An unresolved variable is left alone: nothing says what it is.
    pub(crate) fn undecided_structural_equals(&self, name: &str, ty: &Type) -> Option<String> {
        if crate::builtin_constraints::operation_name(name) != KEY {
            return None;
        }
        let mut parts = Vec::new();
        self.undecided_part(ty, &mut parts, &mut Vec::new())
            .map(|offense| {
                format!(
                    "cannot compare `{ty}` with `{STRUCTURAL_EQUALS}`: {} {offense}, \
                     whose equality is not decided",
                    describe(&parts)
                )
            })
    }

    /// The offense of the first undecided part reached from `ty`, pushing the
    /// path taken. A nominal type already visited is assumed checked: a
    /// recursive declaration terminates. A union rendered by its variants
    /// rather than its name takes the walk each variant takes as a `Con`.
    fn undecided_part(
        &self,
        ty: &Type,
        parts: &mut Vec<Part>,
        visited: &mut Vec<String>,
    ) -> Option<String> {
        match ty {
            Type::Var(_) => None,
            Type::Fun { .. } => Some("is a function".to_string()),
            Type::Record { name, fields } => {
                Self::enter(name, visited);
                self.walk_row(&row_of(fields), parts, visited)
            }
            Type::Con { name, args } => self.undecided_con(name, args, parts, visited),
            Type::Union { name, variants } => {
                Self::enter(name, visited);
                self.walk_types(variants, parts, visited)
            }
        }
    }

    /// The offense of the first undecided part of a named type, or `None` when
    /// every part of it has a decided equality.
    fn undecided_con(
        &self,
        name: &str,
        args: &[Type],
        parts: &mut Vec<Part>,
        visited: &mut Vec<String>,
    ) -> Option<String> {
        match name {
            names::STRING | names::BOOL | names::UNIT => None,
            _ if crate::numeric::Numeric::of_name(name).is_some() => None,
            names::LIST => self.walk_one(args.first(), Part::Element, parts, visited),
            names::MAP => self.walk_one(args.get(1), Part::MapValue, parts, visited),
            names::ANY => Some("is `any`".to_string()),
            names::RESULT => Some("is a `Result`".to_string()),
            names::FIBER | names::CHANNEL | names::PTR | names::GPU_BUFFER => {
                Some("is a runtime handle".to_string())
            }
            _ => self.undecided_nominal(name, args, parts, visited),
        }
    }

    /// A declared union is decided when every variant's payload is; a declared
    /// record when every field is. Anything else is a type the spec table does
    /// not decide.
    fn undecided_nominal(
        &self,
        name: &str,
        args: &[Type],
        parts: &mut Vec<Part>,
        visited: &mut Vec<String>,
    ) -> Option<String> {
        if visited.iter().any(|seen| seen == name) {
            return None;
        }
        if let Some(variants) = self.union_variants.get(name) {
            visited.push(name.to_string());
            return self.walk_union(variants, args, parts, visited);
        }
        if self.ctors.contains_key(name) {
            visited.push(name.to_string());
            return self.walk_row(&self.ctor_fields(name, args), parts, visited);
        }
        Some(format!("is `{name}`"))
    }

    /// Walk one named part: push its hop, and keep it only when the walk
    /// found the offense (the diagnostic names the path to it).
    fn walk_one(
        &self,
        ty: Option<&Type>,
        hop: Part,
        parts: &mut Vec<Part>,
        visited: &mut Vec<String>,
    ) -> Option<String> {
        let ty = ty?;
        parts.push(hop);
        let bad = self.undecided_part(ty, parts, visited);
        if bad.is_none() {
            let _ = parts.pop();
        }
        bad
    }

    /// Walk a record row: each field is a hop on the path.
    fn walk_row(
        &self,
        row: &[Field],
        parts: &mut Vec<Part>,
        visited: &mut Vec<String>,
    ) -> Option<String> {
        row.iter().find_map(|(field, ty)| {
            self.walk_one(Some(ty), Part::Field(field.clone()), parts, visited)
        })
    }

    /// Walk a union: a variant with a payload contributes its fields under the
    /// variant's hop; a payload-free variant is decided by its tag alone.
    fn walk_union(
        &self,
        variants: &[String],
        args: &[Type],
        parts: &mut Vec<Part>,
        visited: &mut Vec<String>,
    ) -> Option<String> {
        variants.iter().find_map(|variant| {
            let row = self.ctor_fields(variant, args);
            if row.is_empty() {
                return None;
            }
            parts.push(Part::Variant(variant.clone()));
            let bad = self.walk_row(&row, parts, visited);
            if bad.is_none() {
                let _ = parts.pop();
            }
            bad
        })
    }

    /// Walk a bare list of types (a `Type::Union`'s variants).
    fn walk_types(
        &self,
        types: &[Type],
        parts: &mut Vec<Part>,
        visited: &mut Vec<String>,
    ) -> Option<String> {
        types
            .iter()
            .find_map(|ty| self.undecided_part(ty, parts, visited))
    }

    /// A constructor's fields instantiated against the owner's type arguments.
    fn ctor_fields(&self, ctor: &str, args: &[Type]) -> Vec<Field> {
        let Some(info) = self.ctors.get(ctor) else {
            return Vec::new();
        };
        let params: HashMap<String, Type> = info
            .type_params
            .iter()
            .zip(args)
            .map(|(param, arg)| (param.clone(), arg.clone()))
            .collect();
        info.fields
            .iter()
            .map(|(field, written)| (field.clone(), type_name_to_type(written, &params)))
            .collect()
    }

    fn enter(name: &str, visited: &mut Vec<String>) {
        if !name.is_empty() && !visited.iter().any(|seen| seen == name) {
            visited.push(name.to_string());
        }
    }
}

/// The offending part the diagnostic names: the innermost hop, then the path
/// to it (`field `onClick``, `field `left` inside variant `Node``).
fn describe(parts: &[Part]) -> String {
    let Some((last, outer)) = parts.split_last() else {
        return "the operand".to_string();
    };
    let head = hop(last);
    if outer.is_empty() {
        return head;
    }
    let path = outer.iter().map(hop).collect::<Vec<_>>().join(" inside ");
    format!("{head} inside {path}")
}

fn hop(part: &Part) -> String {
    match part {
        Part::Field(field) => format!("field `{field}`"),
        Part::Variant(variant) => format!("variant `{variant}`"),
        Part::Element => "list element".to_string(),
        Part::MapValue => "map value".to_string(),
    }
}

#[cfg(test)]
mod tests;
