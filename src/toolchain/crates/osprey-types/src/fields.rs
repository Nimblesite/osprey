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

/// Non-field uses stay inert even after new type bindings are learned.
#[derive(Clone, Copy, Default)]
pub(crate) struct FieldPrefix {
    bindings: usize,
    checked: usize,
    inert: usize,
}

#[cfg(test)]
std::thread_local! {
    pub(crate) static FIELD_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
    pub(crate) fn take_builtin_uses(&mut self) -> Vec<(String, Type)> {
        self.field_uses_prefix = None;
        std::mem::take(&mut self.builtin_uses)
    }

    /// Resolve instantiated relations until no type or pending work changes.
    /// Unresolved receivers remain in the scheme-obligation pipeline.
    pub(crate) fn resolve_field_uses(&mut self) {
        loop {
            let before = self.ctx.bound_count();
            let (start, inert) = self.field_scan_start(before);
            let progressed = start < self.builtin_uses.len() && self.resolve_field_suffix(start);
            let stable = !progressed && self.ctx.bound_count() == before;
            self.field_uses_prefix = Some(FieldPrefix {
                bindings: before,
                checked: if stable {
                    self.builtin_uses.len()
                } else {
                    inert
                },
                inert,
            });
            if stable {
                return;
            }
        }
    }

    fn field_scan_start(&self, bindings: usize) -> (usize, usize) {
        let len = self.builtin_uses.len();
        let prefix = self
            .field_uses_prefix
            .filter(|prefix| prefix.checked <= len)
            .unwrap_or_default();
        let checked = if prefix.bindings == bindings {
            prefix.checked
        } else {
            prefix.inert
        };
        let start = self
            .builtin_uses
            .iter()
            .enumerate()
            .skip(checked)
            .find(|(_, (name, _))| is_field_use(name))
            .map_or(len, |(index, _)| index);
        let inert = if prefix.inert < checked {
            prefix.inert
        } else {
            start
        };
        (start, inert)
    }

    fn resolve_field_suffix(&mut self, checked: usize) -> bool {
        self.field_uses_prefix = None;
        let uses = if checked == 0 {
            std::mem::take(&mut self.builtin_uses)
        } else {
            self.builtin_uses.split_off(checked)
        };
        let mut progressed = false;
        for (name, ty) in uses {
            #[cfg(test)]
            FIELD_VISITS.with(|visits| visits.set(visits.get() + 1));
            let solved = self.resolve_method_use(&name, &ty)
                || parse_obligation(&name).is_some_and(|(write, site, field)| {
                    self.resolve_field_use(write, site, field, &ty)
                });
            progressed |= solved;
            if !solved {
                self.builtin_uses.push((name, ty));
            }
        }
        progressed
    }

    /// The fallback [TYPE-FIELD-ACCESS-NON-RECORD] allows for an untracked
    /// receiver: a field use whose receiver nothing in the program typed
    /// takes the one record that declares the field. An empty list handed to
    /// a generic helper that only reads its elements' fields leaves exactly
    /// that receiver, and without a type the helper's closures have no ABI
    /// (#290). It runs once inference has settled, so a receiver that any
    /// use could still fix is never guessed, and a field two records declare
    /// stays unresolved rather than picking one.
    pub(crate) fn default_untracked_receivers(&mut self) {
        while let Some((receiver, record)) = self.untracked_receiver() {
            let before = self.ctx.bound_count();
            self.push_unify(&receiver, &record);
            self.resolve_field_uses();
            if self.ctx.bound_count() == before {
                return;
            }
        }
    }

    /// The first field use still waiting on an unbound receiver, with the
    /// sole record that declares its field.
    fn untracked_receiver(&mut self) -> Option<(Type, Type)> {
        let uses = self.builtin_uses.clone();
        uses.iter().find_map(|(name, relation)| {
            let (_, _, field) = parse_obligation(name)?;
            let Type::Fun { params, .. } = relation else {
                return None;
            };
            let receiver = self.ctx.apply(params.first()?);
            let unbound = matches!(receiver, Type::Var(_));
            unbound
                .then(|| self.ctx.sole_record_declaring(field))
                .flatten()
                .map(|record| (receiver, record))
        })
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
        self.ctx.settle_head(receiver);
        let receiver = self.ctx.apply(receiver);
        if matches!(receiver, Type::Var(_)) {
            return false;
        }
        match self.resolved_record_field(receiver, field, site) {
            Ok(actual) if write => self.push_assign(&actual, ret),
            Ok(actual) => self.push_unify(ret, &actual),
            Err(error) => self.errors.push(error),
        }
        true
    }

    /// The type of `field` on `receiver` as read from `site`.
    pub(crate) fn resolved_record_field(
        &self,
        receiver: Type,
        field: &str,
        site: &str,
    ) -> Result<Type, TypeError> {
        // The access was written at `site`: inside the module that owns an
        // opaque alias, the receiver is the record the alias stands for.
        let receiver = &self.ctx.exposed_at(receiver, site);
        // Opacity is decided first: an opaque alias `site` cannot read through
        // has no fields to offer, and saying it is "not a struct" would hide why.
        if let Some(hidden) = self.hidden_field(receiver, field, site) {
            return Err(hidden);
        }
        let actual = match receiver {
            Type::Record { fields, .. } => Some(fields.get(field).cloned()),
            Type::Con { name, args } => self
                .ctx
                .record_fields(name, args)
                .map(|mut fields| fields.remove(field)),
            _ => None,
        }
        .ok_or_else(|| {
            TypeError::new(format!(
                "cannot access field '{field}' on non-struct type {receiver}"
            ))
        })?;
        actual.ok_or_else(|| {
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

fn is_field_use(name: &str) -> bool {
    name.starts_with(FIELD_OBLIGATION)
        || name.starts_with(FIELD_WRITE)
        || name.starts_with(crate::methods::OBLIGATION)
}
