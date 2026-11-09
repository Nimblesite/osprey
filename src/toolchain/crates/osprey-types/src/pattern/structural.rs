//! Structural (record, tuple and list) patterns and the row discipline they read.

use super::*;

impl Checker {
    pub(super) fn bind_structural(
        &mut self,
        fields: &[(String, String)],
        open: bool,
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        // Before ANY scrutinee-shaped reasoning, and so uniformly for a concrete
        // row and an erased one: a repeated field name makes the two halves of
        // the compiler disagree about the same pattern.
        if self.reject_duplicate_fields(fields) {
            self.bind_fresh(fields, local);
            return;
        }
        let dp = self.ctx.expose(disc);
        if dp.is_named(names::ANY) {
            for (_, binder) in fields {
                if !binder.is_empty() {
                    local.insert(binder.clone(), Scheme::mono(Type::any()));
                }
            }
            return;
        }
        let Some(row) = self.structural_row(&dp, disc) else {
            // Raw inference variables have no user-facing spelling; the
            // guidance for an unresolved scrutinee is the annotation itself.
            let found = match &dp {
                Type::Var(_) => ": annotate the value it matches".to_string(),
                other => format!(", found {other}"),
            };
            self.errors.push(TypeError::new(format!(
                "a structural pattern needs a record or `any` scrutinee{found}"
            )));
            self.bind_fresh(fields, local);
            return;
        };
        if let Some(hidden) = self.hidden_row(&dp, fields) {
            self.errors.push(hidden);
            self.bind_fresh(fields, local);
            return;
        }
        self.check_row_selects(fields, open, &dp, &row);
        for (fname, binder) in fields {
            if binder.is_empty() {
                continue;
            }
            let ft = row.get(fname).cloned().unwrap_or_else(|| self.ctx.fresh());
            local.insert(binder.clone(), Scheme::mono(ft));
        }
    }

    /// The field row a concrete scrutinee exposes to a structural pattern: a
    /// record type carries it directly; a nominal record annotation arrives as
    /// a bare `Con`, so its declared row is instantiated and tied to the
    /// discriminant exactly as a constructor pattern would.
    fn structural_row(&mut self, dp: &Type, disc: &Type) -> Option<BTreeMap<String, Type>> {
        if let Type::Record { fields, .. } = dp {
            return Some(fields.clone());
        }
        let Type::Con { name, .. } = dp else {
            return None;
        };
        let name = name.clone();
        if !self.ctors.get(&name).is_some_and(|i| i.owner_is_record) {
            return None;
        }
        let (_, declared, owner, _) = self.ctor_instance(&name)?;
        let declared_map: BTreeMap<String, Type> = declared.into_iter().collect();
        self.push_unify(
            &Type::Record {
                name: owner,
                fields: declared_map.clone(),
            },
            disc,
        );
        Some(declared_map)
    }

    /// Reject a structural pattern that names the same field twice.
    ///
    /// A duplicate binds one name from one slot and tells the row test nothing
    /// new, and the two halves of the compiler counted it differently: the
    /// closed-row check below compared the pattern's VECTOR LENGTH against the
    /// row's, which a duplicate satisfies while a real field is missing, while
    /// the runtime row test compares NAME SETS ([`candidate_rows`]), which a
    /// duplicate collapses. So `{ x, x }` selected a concrete `Pair { x, y }`
    /// and then MISSED the very same value once erased to `any` — erasure
    /// changing whether a pattern matches is the one thing [TYPE-ANY] must
    /// never do. Rejecting the duplicate outright removes the disagreement at
    /// its source rather than teaching both sides the same wrong rule.
    ///
    /// Returns whether an error was reported.
    /// Bind every named binder in `fields` to a fresh variable — the recovery
    /// taken whenever the scrutinee cannot supply field types, so the arm body
    /// still type-checks against the names it wrote instead of cascading.
    fn bind_fresh(&mut self, fields: &[(String, String)], local: &mut TypeEnv) {
        for (_, binder) in fields {
            if !binder.is_empty() {
                let fv = self.ctx.fresh();
                local.insert(binder.clone(), Scheme::mono(fv));
            }
        }
    }

    fn reject_duplicate_fields(&mut self, fields: &[(String, String)]) -> bool {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let dup = fields
            .iter()
            .find(|(fname, _)| !seen.insert(fname.as_str()))
            .map(|(fname, _)| fname.clone());
        match dup {
            Some(fname) => {
                self.errors.push(TypeError::new(format!(
                    "structural pattern names `{fname}` more than once; \
                     each field may be bound at most once"
                )));
                true
            }
            None => false,
        }
    }

    /// Reject a structural arm that can never select its concrete scrutinee:
    /// naming a field the row lacks, or a closed pattern that does not name
    /// the whole row ([TYPE-ROW]: closed rows unify only when their names
    /// agree exactly).
    fn check_row_selects(
        &mut self,
        fields: &[(String, String)],
        open: bool,
        dp: &Type,
        row: &BTreeMap<String, Type>,
    ) {
        for (fname, _) in fields {
            if !row.contains_key(fname) {
                self.errors.push(TypeError::new(format!(
                    "structural pattern names `{fname}`, but {dp} has no such field"
                )));
                return;
            }
        }
        // Count DISTINCT names, never the vector length: a closed row is
        // selected by name-set equality, so length is the wrong measure even
        // though `reject_duplicate_fields` now keeps the two in step.
        let named: BTreeSet<&str> = fields.iter().map(|(f, _)| f.as_str()).collect();
        if !open && named.len() != row.len() {
            let missing: Vec<&str> = row
                .keys()
                .filter(|k| !fields.iter().any(|(f, _)| f == *k))
                .map(String::as_str)
                .collect();
            self.errors.push(TypeError::new(format!(
                "a closed structural pattern must name the whole row of {dp}; \
                 missing {}: add `..` to open it",
                missing.join(", ")
            )));
        }
    }

    /// A list pattern unifies the discriminant with `List<E>` for a fresh element
    /// type `E`, binds each prefix element against `E`, and binds the `...rest`
    /// tail (when present) as `List<E>` — the same element type, since `drop`
    /// yields a suffix of the same list. Implements [TYPE-LIST-PATTERNS].
    pub(super) fn bind_list_pattern(
        &mut self,
        elements: &[Pattern],
        rest: Option<&str>,
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        let elem = self.ctx.fresh();
        let list_ty = Type::list(elem.clone());
        self.push_unify(&list_ty, disc);
        for el in elements {
            self.bind_pattern(el, &elem, local);
        }
        if let Some(name) = rest {
            local.insert(name.to_string(), Scheme::mono(list_ty));
        }
    }

    /// A bare identifier pattern is either a variant name (matches that variant
    /// and binds nothing, payload or not) or a fresh variable binding.
    pub(super) fn bind_binding(&mut self, name: &str, disc: &Type, local: &mut TypeEnv) {
        // `Success`/`Error` over a real `Result` always mean the built-in
        // variant, even when a user union shadows those names: match the
        // variant, bind nothing.
        if is_result_variant(name) && is_result(&self.ctx.expose(disc)) {
            return;
        }
        if self.names_variant(name) {
            if let Some((args, declared, owner, is_record)) = self.ctor_instance(name) {
                let fields = declared.into_iter().collect();
                self.push_unify(&ctor_owner_ty(owner, args, fields, is_record), disc);
                return;
            }
        }
        if let Some(owner) = self.unknown_variant_owner(name, disc) {
            self.errors.push(TypeError::new(format!(
                "unknown variant in match expression: variant `{name}` is not defined in type `{owner}`"
            )));
            return;
        }
        local.insert(name.to_string(), Scheme::mono(disc.clone()));
    }
}
