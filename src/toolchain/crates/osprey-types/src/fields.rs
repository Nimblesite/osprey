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

/// An index read whose target is not typed yet: `(target, index) -> result`.
pub(crate) const INDEX_OBLIGATION: &str = "$index";

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

/// Whether field resolution can ever solve the obligation `name`: a method,
/// index or field relation. Every other obligation is an operator's.
fn resolvable(name: &str) -> bool {
    name.starts_with(crate::methods::OBLIGATION)
        || name == INDEX_OBLIGATION
        || name.starts_with(FIELD_OBLIGATION)
        || name.starts_with(FIELD_WRITE)
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
            let checked = self.checked_field_prefix(before);
            if checked == self.builtin_uses.len() {
                return;
            }
            let progressed = self.resolve_field_suffix(checked);
            if !progressed && self.ctx.bound_count() == before {
                self.field_uses_prefix = Some((before, self.builtin_uses.len()));
                return;
            }
        }
    }

    pub(crate) fn checked_field_prefix(&self, bindings: usize) -> usize {
        self.field_uses_prefix
            .filter(|(bound, count)| *bound == bindings && *count <= self.builtin_uses.len())
            .map_or(0, |(_, count)| count)
    }

    /// Try every relation after the checked prefix. The operator obligations
    /// before the first relation stay where they are, unvisited: nothing here
    /// can solve them, and a large program holds far more of them.
    fn resolve_field_suffix(&mut self, checked: usize) -> bool {
        self.field_uses_prefix = None;
        let start = self
            .builtin_uses
            .iter()
            .skip(checked)
            .position(|(name, _)| resolvable(name))
            .map_or(self.builtin_uses.len(), |at| checked + at);
        let uses = if start == 0 {
            std::mem::take(&mut self.builtin_uses)
        } else {
            self.builtin_uses.split_off(start)
        };
        let mut progressed = false;
        for (name, ty) in uses {
            let solved = resolvable(&name) && self.resolve_relation(&name, &ty);
            progressed |= solved;
            if !solved {
                self.builtin_uses.push((name, ty));
            }
        }
        progressed
    }

    /// Solve one method, index or field relation once its receiver is known.
    fn resolve_relation(&mut self, name: &str, ty: &Type) -> bool {
        #[cfg(test)]
        FIELD_VISITS.with(|visits| visits.set(visits.get() + 1));
        self.resolve_method_use(name, ty)
            || (name == INDEX_OBLIGATION && self.resolve_index_use(ty))
            || parse_obligation(name)
                .is_some_and(|(write, site, field)| self.resolve_field_use(write, site, field, ty))
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
        self.builtin_uses.iter().find_map(|(name, relation)| {
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

    fn resolve_index_use(&mut self, relation: &Type) -> bool {
        let Type::Fun { params, ret } = relation else {
            return false;
        };
        let [target, index] = params.as_slice() else {
            return false;
        };
        self.ctx.settle_head(target);
        match self.index_result(target, index) {
            Some(result) => self.push_unify(ret, &result),
            None => return false,
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
            Type::Con { name, args } => self.ctx.record_field(name, args, field).ok(),
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
