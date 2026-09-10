//! Field constraints carried through ordinary HM schemes.
//!
//! Implements [TYPE-GENERICS-FN] and [TYPE-FIELD-ACCESS-NON-RECORD]: a generic
//! accessor's result must remain related to the record supplied at each call.

use crate::check::Checker;
use crate::error::TypeError;
use crate::ty::Type;

const FIELD_OBLIGATION: &str = "$field:";

pub(crate) fn obligation_name(field: &str) -> String {
    format!("{FIELD_OBLIGATION}{field}")
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
                    || name
                        .strip_prefix(FIELD_OBLIGATION)
                        .is_some_and(|field| self.resolve_field_use(field, &ty));
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

    fn resolve_field_use(&mut self, field: &str, relation: &Type) -> bool {
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
        match self.resolved_record_field(&receiver, field) {
            Ok(actual) => self.push_unify(ret, &actual),
            Err(error) => self.errors.push(error),
        }
        true
    }

    pub(crate) fn resolved_record_field(
        &self,
        receiver: &Type,
        field: &str,
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
        fields.get(field).cloned().ok_or_else(|| {
            TypeError::new(format!("record type `{receiver}` has no field `{field}`"))
        })
    }
}
