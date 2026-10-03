//! Field constraints carried through ordinary HM schemes.
//!
//! Implements [TYPE-GENERICS-FN] and [TYPE-FIELD-ACCESS-NON-RECORD]: a generic
//! accessor's result must remain related to the record supplied at each call.
//! Implements [MODULES-OPAQUE-TYPES]: the obligation also remembers WHERE the
//! access was written, so an opaque record's field stays readable through an
//! accessor its own module exports and hidden from everything else.

use crate::check::Checker;
use crate::error::TypeError;
use crate::ty::Type;

const FIELD_OBLIGATION: &str = "$field:";
/// A non-destructive update's write of one field ([TYPE-RECORD-UPDATE]): the
/// same deferral as a read, resolved as an assignment INTO the field.
const FIELD_WRITE: &str = "$write:";

/// `$field:{site}:{field}` — `site` is the linkage name of the top-level
/// declaration that wrote the access ([`Checker::site`]).
pub(crate) fn obligation_name(site: &str, field: &str) -> String {
    format!("{FIELD_OBLIGATION}{site}:{field}")
}

/// `$write:{site}:{field}` — the relation's result is the value written.
pub(crate) fn write_obligation_name(site: &str, field: &str) -> String {
    format!("{FIELD_WRITE}{site}:{field}")
}

/// `(is_write, site, field)` of a field obligation; `None` for any other.
fn parse_obligation(name: &str) -> Option<(bool, &str, &str)> {
    let (write, rest) = match name.strip_prefix(FIELD_OBLIGATION) {
        Some(rest) => (false, rest),
        None => (true, name.strip_prefix(FIELD_WRITE)?),
    };
    let (site, field) = rest.split_once(':')?;
    Some((write, site, field))
}

impl Checker {
    /// Resolve instantiated relations until no type or pending work changes.
    /// Unresolved receivers remain in the scheme-obligation pipeline.
    pub(crate) fn resolve_field_uses(&mut self) {
        loop {
            let before = self.ctx.bound_count();
            let uses = std::mem::take(&mut self.builtin_uses);
            let mut progressed = false;
            for (name, ty) in uses {
                let solved = self.resolve_method_use(&name, &ty)
                    || parse_obligation(&name).is_some_and(|(write, site, field)| {
                        self.resolve_field_use(write, site, field, &ty)
                    });
                progressed |= solved;
                if !solved {
                    self.builtin_uses.push((name, ty));
                }
            }
            if !progressed && self.ctx.bound_count() == before {
                return;
            }
        }
    }

    /// A read unifies the field with the relation's result; a write assigns
    /// the result (the value written) into the field, so an update keeps the
    /// one-way rules of an assignment — `any` erasure, implicit `Success` —
    /// however late the record becomes known.
    fn resolve_field_use(&mut self, write: bool, site: &str, field: &str, relation: &Type) -> bool {
        let Type::Fun { params, ret } = relation else {
            return false;
        };
        let [receiver] = params.as_slice() else {
            return false;
        };
        let receiver = self.ctx.apply(receiver);
        if matches!(receiver, Type::Var(_)) {
            return false;
        }
        match self.resolved_record_field(&receiver, field, site) {
            Ok(actual) if write => self.push_assign(&actual, ret),
            Ok(actual) => self.push_unify(ret, &actual),
            Err(error) => self.errors.push(error),
        }
        true
    }

    /// The type of `field` on `receiver` as read from `site`.
    pub(crate) fn resolved_record_field(
        &self,
        receiver: &Type,
        field: &str,
        site: &str,
    ) -> Result<Type, TypeError> {
        let fields = match receiver {
            Type::Record { fields, .. } => Some(fields.clone()),
            Type::Con { name, args } => self.ctx.record_fields(name, args),
            _ => None,
        }
        .ok_or_else(|| {
            TypeError::new(format!(
                "cannot access field '{field}' on non-struct type {receiver}"
            ))
        })?;
        if let Some(hidden) = self.hidden_field(receiver, field, site) {
            return Err(hidden);
        }
        fields.get(field).cloned().ok_or_else(|| {
            // A declared record is named by its declaration, not by the row it
            // happens to carry, so the message reads as the author wrote it.
            let shown = match receiver {
                Type::Record { name, .. } if !name.is_empty() => name.clone(),
                other => other.to_string(),
            };
            TypeError::new(format!("record type `{shown}` has no field `{field}`"))
        })
    }

    /// The rejection for reading `field` of an opaque record from a `site`
    /// outside the record's module; `None` when the read is allowed.
    pub(crate) fn hidden_field(
        &self,
        receiver: &Type,
        field: &str,
        site: &str,
    ) -> Option<TypeError> {
        let (Type::Record { name, .. } | Type::Con { name, .. }) = receiver else {
            return None;
        };
        let owner = osprey_ast::symbol::parent(name)?;
        let hidden = self.opaque_types.contains(name) && !osprey_ast::symbol::encloses(name, site);
        hidden.then(|| {
            TypeError::new(format!(
                "field `{field}` of opaque type `{name}` is hidden outside module `{owner}`"
            ))
        })
    }
}
