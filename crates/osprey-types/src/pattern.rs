//! Pattern inference and match exhaustiveness.
//!
//! Binding a constructor pattern unifies the discriminant with the
//! constructor's owner type, so the discriminant's type arguments flow into the
//! bound field types (`Success { value }` over `Result<int, E>` binds
//! `value : int`). Exhaustiveness is enforced only where the checker can decide
//! it with confidence — `bool` and known union/`Result` discriminants — and is
//! otherwise deferred to a catch-all, so it never reports a false positive.

use crate::check::Checker;
use crate::convert::type_expr_to_type;
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::{names, Scheme, Type};
use osprey_ast::{Expr, MatchArm, Pattern};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

fn is_result(t: &Type) -> bool {
    matches!(t, Type::Con { name, .. } if name == names::RESULT)
}

/// Whether `name` is one of the built-in `Result` variant constructors. A
/// user-declared variant may reuse these names (shadowing the builtin for
/// construction), but a pattern over a discriminant that *is* a `Result` always
/// means the built-in variant.
fn is_result_variant(name: &str) -> bool {
    name == names::SUCCESS || name == names::ERROR
}

/// An initial-uppercase identifier reads as a constructor/variant; a lower-case
/// one reads as an ordinary variable binding.
fn starts_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

impl Checker {
    pub(crate) fn infer_match(&mut self, value: &Expr, arms: &[MatchArm], env: &TypeEnv) -> Type {
        let disc = self.infer_expr(value, env);
        let result = self.ctx.fresh();
        for arm in arms {
            let body_ty = self.infer_arm(arm, &disc, env);
            self.push_unify(&result, &body_ty);
        }
        self.check_exhaustive(&disc, arms);
        self.check_redundant_arms(arms, &disc);
        result
    }

    /// Infer one arm's body type after binding its pattern against the exact
    /// discriminant type.
    fn infer_arm(&mut self, arm: &MatchArm, disc: &Type, env: &TypeEnv) -> Type {
        let mut local = env.child();
        self.bind_pattern(&arm.pattern, disc, &mut local);
        self.infer_expr(&arm.body, &local)
    }

    fn bind_pattern(&mut self, pattern: &Pattern, disc: &Type, local: &mut TypeEnv) {
        if self.reject_reading_erased(pattern, disc, local) {
            return;
        }
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Binding(name) => self.bind_binding(name, disc, local),
            Pattern::Literal(expr) => {
                let lt = self.infer_expr(expr, local);
                self.push_unify(disc, &lt);
            }
            Pattern::TypeAnnotated { name, ty } => {
                let t = type_expr_to_type(ty, &HashMap::new());
                local.insert(name.clone(), Scheme::mono(t));
            }
            Pattern::Structural { fields, open } => {
                self.bind_structural(fields, *open, disc, local);
            }
            Pattern::Constructor {
                name,
                fields,
                sub_patterns,
            } => self.bind_constructor(name, fields, sub_patterns, disc, local),
            Pattern::List { elements, rest } => {
                self.bind_list_pattern(elements, rest.as_deref(), disc, local);
            }
        }
    }

    /// Every pattern that READS a value's shape without a runtime row test — a
    /// literal compare, a variant tag, a list length, a type-annotated binding
    /// that simply renames the static type — is an unchecked read over an
    /// erased `any`: the same recovery [TYPE-ANY] rejects at annotations,
    /// respelled as a pattern (`s: string => length(s)` compiled and read the
    /// word with no test at all). Only a structural arm carries a row test, so
    /// everything else is rejected toward one. `Success`/`Error` stay legal:
    /// auto-wrap binds the erased value whole and reads nothing through it.
    fn reject_reading_erased(
        &mut self,
        pattern: &Pattern,
        disc: &Type,
        local: &mut TypeEnv,
    ) -> bool {
        if !self.ctx.prune(disc).is_named(names::ANY) {
            return false;
        }
        let offending = match pattern {
            Pattern::Literal(_) => "a literal pattern",
            Pattern::List { .. } => "a list pattern",
            Pattern::TypeAnnotated { .. } => "a type-annotated binding",
            Pattern::Constructor { name, .. } if !is_result_variant(name) => {
                "a constructor pattern"
            }
            Pattern::Binding(name) if self.ctors.get(name).is_some_and(|i| i.fields.is_empty()) => {
                "a variant pattern"
            }
            _ => return false,
        };
        // Bind the pattern's names as `any` so one mistake does not cascade
        // into unknown-identifier noise in the arm body.
        for binder in pattern_binder_names(pattern) {
            local.insert(binder, Scheme::mono(Type::any()));
        }
        self.errors.push(TypeError::new(format!(
            "{offending} cannot read an erased `any`: match its structure instead"
        )));
        true
    }

    /// Bind a structural row pattern ([PATTERN-STRUCTURAL]) or its positional
    /// tuple spelling ([PATTERN-TUPLE]). Over a concrete record the row is
    /// checked here — a closed pattern names the exact row, an open one a
    /// subset — and each binder takes its declared field type. Over `any`
    /// every binder is itself `any`: the row is unknown until run time, so a
    /// fresh-variable binder would let the body read the field at any type it
    /// pleased — recovery by pattern, the hole [TYPE-ANY] deletes.
    fn bind_structural(
        &mut self,
        fields: &[(String, String)],
        open: bool,
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        let dp = self.ctx.prune(disc);
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
            for (_, binder) in fields {
                if !binder.is_empty() {
                    let fv = self.ctx.fresh();
                    local.insert(binder.clone(), Scheme::mono(fv));
                }
            }
            return;
        };
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
        if !open && fields.len() != row.len() {
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
    fn bind_list_pattern(
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

    /// A bare identifier pattern is either a nullary constructor (matches that
    /// variant) or a fresh variable binding.
    fn bind_binding(&mut self, name: &str, disc: &Type, local: &mut TypeEnv) {
        // `Success`/`Error` over a real `Result` always mean the built-in
        // variant, even when a user union shadows those names: match the
        // variant, bind nothing.
        if is_result_variant(name) && is_result(&self.ctx.prune(disc)) {
            return;
        }
        if self.ctors.get(name).is_some_and(|i| i.fields.is_empty()) {
            if let Some((args, _f, owner, is_record)) = self.ctor_instance(name) {
                let owner_ty = nullary_owner_ty(owner, args, is_record);
                self.push_unify(&owner_ty, disc);
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

    /// When `name` looks like a variant (capitalised) but the discriminant is a
    /// known union that has no such variant, return the union's name — a
    /// lower-case identifier is an ordinary catch-all binding instead.
    fn unknown_variant_owner(&mut self, name: &str, disc: &Type) -> Option<String> {
        if !starts_uppercase(name) {
            return None;
        }
        match self.ctx.prune(disc) {
            Type::Con { name: owner, .. } => {
                let variants = self.union_variants.get(&owner)?;
                if variants.iter().any(|v| v == name) {
                    None
                } else {
                    Some(owner)
                }
            }
            _ => None,
        }
    }

    fn bind_constructor(
        &mut self,
        name: &str,
        fields: &[String],
        sub_patterns: &[Pattern],
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        // `Success { value }` / `Error { message }` over a real `Result` always
        // bind the built-in variant's fields, even when a user union shadows
        // those constructor names.
        if is_result_variant(name) && is_result(&self.ctx.prune(disc)) {
            self.bind_result_fields(fields, disc, local);
            return;
        }
        let Some((args, declared, owner, is_record)) = self.ctor_instance(name) else {
            self.errors.push(TypeError::new(format!(
                "unknown constructor `{name}` in match pattern"
            )));
            for f in fields {
                let fv = self.ctx.fresh();
                local.insert(f.clone(), Scheme::mono(fv));
            }
            return;
        };
        // `Result` patterns (`Success`/`Error`) auto-wrap a non-Result
        // discriminant: `match a + b { Success { value } => .. }` over a `string`
        // binds `value : string` (the match auto-wrap rule: any value may be
        // matched as if wrapped in `Success`). This also lets validated record
        // constructions be matched without a real Result.
        if owner == names::RESULT {
            self.reject_plain_result_default(fields, disc);
            self.bind_result_fields(fields, disc, local);
            return;
        }
        let declared_map: BTreeMap<String, Type> = declared.iter().cloned().collect();
        // Tie the discriminant's type arguments to this constructor's owner.
        let owner_ty = if is_record {
            Type::Record {
                name: owner,
                fields: declared_map.clone(),
            }
        } else {
            Type::con(owner, args)
        };
        self.push_unify(&owner_ty, disc);

        // Named field destructure: `Ctor { a, b }`.
        for fname in fields {
            let ft = declared_map
                .get(fname)
                .cloned()
                .unwrap_or_else(|| self.ctx.fresh());
            local.insert(fname.clone(), Scheme::mono(ft));
        }
        // Positional destructure: `Ctor(p0, p1)`.
        for (i, sub) in sub_patterns.iter().enumerate() {
            self.reject_nested_pattern(sub);
            let field_ty = declared.get(i).map(|(_, t)| t.clone());
            let target = field_ty.unwrap_or_else(|| self.ctx.fresh());
            self.bind_pattern(sub, &target, local);
        }
    }

    /// A positional slot destructures to a binder or `_` and nothing else.
    /// Codegen binds a slot by index and has no register to match a deeper
    /// pattern against, so an unsupported sub-pattern would be silently
    /// discarded and the arm would behave as `Ctor(_, _)`. The ML flavor
    /// rejects the same shape in its parser ([FLAVOR-ML-PATTERN-GROUP]); this
    /// is the Default flavor's half of that rule.
    fn reject_nested_pattern(&mut self, sub: &Pattern) {
        if matches!(sub, Pattern::Binding(_) | Pattern::Wildcard) {
            return;
        }
        self.errors.push(TypeError::new(
            "nested constructor patterns are not supported; \
             bind the payload and match it in a second expression"
                .to_owned(),
        ));
    }

    /// Bind the built-in `Result` pattern fields against `disc`: `value` is the
    /// success payload (the discriminant itself when it is not a `Result` — the
    /// match auto-wrap rule), `message` is the error string.
    /// `?:` is an explicit Result handling operation, so it does NOT get the
    /// ordinary `Success`-arm auto-wrap: [PATTERN-RESULT-DEFAULT] states that
    /// its scrutinee must be a `Result` and that it "never reinterprets a plain
    /// value as `Success`". Inheriting auto-wrap made `5 ?: -1` well-typed with
    /// an unreachable fallback. Only the desugarer's unspellable payload binder
    /// distinguishes the two, since both are a `Success`/`Error` match.
    ///
    /// An unresolved scrutinee is left alone: this checker unifies eagerly, so a
    /// type variable here may still become a `Result` later.
    fn reject_plain_result_default(&mut self, fields: &[String], disc: &Type) {
        if !fields
            .iter()
            .any(|f| f == osprey_ast::RESULT_DEFAULT_PAYLOAD)
        {
            return;
        }
        let pruned = self.ctx.prune(disc);
        if is_result(&pruned) || matches!(pruned, Type::Var(_)) {
            return;
        }
        self.errors.push(TypeError::new(format!(
            "`?:` needs a Result on its left, found {pruned}"
        )));
    }

    fn bind_result_fields(&mut self, fields: &[String], disc: &Type, local: &mut TypeEnv) {
        // An UNRESOLVED discriminant is not a candidate for the auto-wrap rule
        // below — auto-wrap answers "what does `Success { value }` mean over a
        // value that is known not to be a Result", and nothing is known here
        // yet. A `Success`/`Error` pattern is itself the evidence, so pin the
        // scrutinee to a Result with an open payload. Binding the whole
        // variable as the payload instead detached the two: once the scrutinee
        // later became `Result<int, MathError>` — as a deferred arithmetic
        // operand does ([`crate::expr::Checker::deferred_arith`]) — `value` was
        // still the Result, and `Success { value: value }` failed with `cannot
        // unify Result<int, MathError> with int`.
        if matches!(self.ctx.prune(disc), Type::Var(_)) {
            let open = Type::result(self.ctx.fresh(), self.ctx.fresh());
            self.push_unify(&open, disc);
        }
        let dp = self.ctx.prune(disc);
        let ok = match &dp {
            Type::Con { name, args } if name == names::RESULT && !args.is_empty() => {
                args.first().cloned().unwrap_or_else(|| dp.clone())
            }
            _ => dp.clone(),
        };
        for fname in fields {
            // The desugarer's unspellable `?:` binder names the SAME success
            // payload `value` does. Leaving it to the fresh-variable fallback
            // detached the payload from the fallback expression, so
            // `listGet([1, 2, 3], 0) ?: 9.5` type-checked and only failed in the
            // backend ("match arms disagree on type"), and an empty literal's
            // element type was never resolved at all [PATTERN-RESULT-DEFAULT].
            let ft = match fname.as_str() {
                "value" | osprey_ast::RESULT_DEFAULT_PAYLOAD => ok.clone(),
                "message" => Type::string(),
                _ => self.ctx.fresh(),
            };
            local.insert(fname.clone(), Scheme::mono(ft));
        }
    }

    /// Enforce exhaustiveness where it is unambiguous: `bool` needs both
    /// constructors; a known union/`Result` needs every variant — unless a
    /// catch-all arm is present.
    fn check_exhaustive(&mut self, disc: &Type, arms: &[MatchArm]) {
        if arms.iter().any(|a| self.is_catch_all(&a.pattern)) {
            return;
        }
        let dp = self.ctx.apply(disc);
        match &dp {
            t if t.is_named(names::BOOL) => self.check_bool_exhaustive(arms),
            // An erased value's row is not known until run time, so no set of
            // structural arms can cover it ([TYPE-ANY], [TYPE-MATCH-EXHAUSTIVE]).
            t if t.is_named(names::ANY) => self.errors.push(TypeError::new(
                "a match over `any` is never exhaustive: add a catch-all arm",
            )),
            Type::Con { name, .. } if self.union_variants.contains_key(name) => {
                let all = self.union_variants.get(name).cloned().unwrap_or_default();
                let covered: HashSet<String> = arms
                    .iter()
                    .filter_map(|a| self.pattern_ctor_name(&a.pattern))
                    .collect();
                let missing: Vec<String> = all
                    .iter()
                    .filter(|v| !covered.contains(*v))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    self.errors.push(TypeError::new(format!(
                        "non-exhaustive match on `{name}`: missing {}",
                        missing.join(", ")
                    )));
                }
            }
            _ => {}
        }
    }

    fn check_bool_exhaustive(&mut self, arms: &[MatchArm]) {
        let mut has_true = false;
        let mut has_false = false;
        for arm in arms {
            if let Pattern::Literal(expr) = &arm.pattern {
                if let Expr::Bool(b) = expr.as_ref() {
                    has_true |= *b;
                    has_false |= !*b;
                }
            }
        }
        if !(has_true && has_false) {
            self.errors.push(TypeError::new(
                "non-exhaustive match on `bool`: needs both true and false",
            ));
        }
    }

    /// Flag arms that can never run: any arm after an irrefutable (catch-all)
    /// arm, a repeated constructor/variant arm, and a structural arm an
    /// earlier structural arm shadows — a `..`-opened row covers every row
    /// extending it, and a closed row repeats only itself; arm order must
    /// never resolve that silently ([PATTERN-STRUCTURAL]). Over a concrete
    /// record scrutinee a well-typed structural arm always selects, so it also
    /// ends the reachable arms. Implements [TYPE-MATCH-EXHAUSTIVE].
    fn check_redundant_arms(&mut self, arms: &[MatchArm], disc: &Type) {
        let concrete_row = {
            let dp = self.ctx.apply(disc);
            self.is_row_scrutinee(&dp)
        };
        let mut covered_by: Option<&str> = None;
        let mut seen: HashSet<String> = HashSet::new();
        let mut seen_rows: Vec<(BTreeSet<&str>, bool)> = Vec::new();
        for arm in arms {
            if let Some(earlier) = covered_by {
                self.errors.push(TypeError::new(format!(
                    "unreachable match arm: an earlier {earlier} already covers every case"
                )));
                continue;
            }
            if let Some(name) = self.pattern_ctor_name(&arm.pattern) {
                if !seen.insert(name.clone()) {
                    self.errors.push(TypeError::new(format!(
                        "unreachable match arm: variant `{name}` is already matched by an earlier arm"
                    )));
                }
            }
            if let Pattern::Structural { fields, open } = &arm.pattern {
                let names: BTreeSet<&str> = fields.iter().map(|(f, _)| f.as_str()).collect();
                if let Some(shadow) = shadowing_row(&seen_rows, &names) {
                    self.errors
                        .push(TypeError::new(format!("unreachable match arm: {shadow}")));
                }
                seen_rows.push((names, *open));
                // A well-typed structural arm over a concrete record always
                // selects, so nothing after it can run.
                covered_by = concrete_row.then_some("structural arm");
                continue;
            }
            covered_by = self.is_irrefutable(&arm.pattern).then_some("catch-all");
        }
    }

    /// Whether a concrete scrutinee exposes a field row to structural arms — a
    /// record type, or a nominal record referenced by name. The pure query
    /// behind [`Self::structural_row`], for reachability decisions.
    fn is_row_scrutinee(&self, dp: &Type) -> bool {
        match dp {
            Type::Record { .. } => true,
            Type::Con { name, .. } => self.ctors.get(name).is_some_and(|i| i.owner_is_record),
            _ => false,
        }
    }

    /// Whether an arm absorbs every remaining case for *exhaustiveness*: a
    /// wildcard, a typed binding (a single `n: Int` arm is treated as a catch-all
    /// so it is not flagged non-exhaustive), or a genuine variable binding. A
    /// capitalised bare name is a (possibly mis-spelled) constructor attempt — not
    /// a catch-all — so the missing/unknown-variant path reports it.
    fn is_catch_all(&self, pattern: &Pattern) -> bool {
        match pattern {
            Pattern::Wildcard | Pattern::TypeAnnotated { .. } => true,
            Pattern::Binding(name) => self.is_variable_binding(name),
            _ => false,
        }
    }

    /// Irrefutable patterns for *reachability*: only a wildcard or a genuine
    /// variable binding truly covers every remaining value. A typed binding is a
    /// type *test* (refutable), so it does not make later arms unreachable.
    fn is_irrefutable(&self, pattern: &Pattern) -> bool {
        match pattern {
            Pattern::Wildcard => true,
            Pattern::Binding(name) => self.is_variable_binding(name),
            _ => false,
        }
    }

    /// A bare binding is a genuine variable (not a variant) when it is lower-case
    /// and not a known nullary constructor.
    fn is_variable_binding(&self, name: &str) -> bool {
        !starts_uppercase(name) && self.ctors.get(name).is_none_or(|i| !i.fields.is_empty())
    }

    /// The variant a pattern covers for exhaustiveness, if any: an explicit
    /// constructor, a bare built-in `Result` variant (`Success`/`Error`, whose
    /// fields are non-empty yet still name a variant), or a nullary-constructor
    /// binding.
    fn pattern_ctor_name(&self, pattern: &Pattern) -> Option<String> {
        match pattern {
            Pattern::Constructor { name, .. } => Some(name.clone()),
            Pattern::Binding(name) if is_result_variant(name) => Some(name.clone()),
            Pattern::Binding(name) if self.ctors.get(name).is_some_and(|i| i.fields.is_empty()) => {
                Some(name.clone())
            }
            _ => None,
        }
    }
}

/// The description of an earlier structural arm that makes one carrying
/// `names` unreachable: an open row it extends (an open `{ x, .. }` selects
/// every row carrying `x`), or an identical closed row. Rows are compared as
/// ordered sets so the diagnostic spelling is deterministic.
fn shadowing_row(seen: &[(BTreeSet<&str>, bool)], names: &BTreeSet<&str>) -> Option<String> {
    seen.iter().find_map(|(prev, open)| {
        if *open && prev.is_subset(names) {
            let spelled = prev.iter().copied().collect::<Vec<_>>().join(", ");
            Some(format!(
                "an earlier `{{ {spelled}, .. }}` arm already covers this row"
            ))
        } else if !*open && prev == names {
            Some("this row is already matched by an earlier arm".to_string())
        } else {
            None
        }
    })
}

/// Every name a pattern would bind — the cascade-avoidance binding set for a
/// rejected arm over an erased scrutinee.
fn pattern_binder_names(pattern: &Pattern) -> Vec<String> {
    match pattern {
        Pattern::TypeAnnotated { name, .. } | Pattern::Binding(name) => vec![name.clone()],
        Pattern::Constructor {
            fields,
            sub_patterns,
            ..
        } => fields
            .iter()
            .cloned()
            .chain(sub_patterns.iter().flat_map(pattern_binder_names))
            .collect(),
        Pattern::List { elements, rest } => elements
            .iter()
            .flat_map(pattern_binder_names)
            .chain(rest.iter().cloned())
            .collect(),
        Pattern::Structural { fields, .. } => fields
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .map(|(_, b)| b.clone())
            .collect(),
        Pattern::Wildcard | Pattern::Literal(_) => Vec::new(),
    }
}

fn nullary_owner_ty(owner: String, args: Vec<Type>, is_record: bool) -> Type {
    if is_record {
        Type::Record {
            name: owner,
            fields: BTreeMap::new(),
        }
    } else {
        Type::con(owner, args)
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{check, ok};

    #[test]
    fn structural_pattern_binds_record_fields() {
        ok("type Point = { x: int, y: int }\n\
            fn getx(p: Point) -> int = match p {\n\
              { x, y } => (x + y) ?: 0\n\
            }\n");
        // An UNRESOLVED scrutinee refuses a structural arm: the old contract
        // bound its fields as fresh variables, which let the body read each
        // field at whatever type it pleased — recovery through a pattern,
        // with no annotation to reject ([TYPE-ANY]).
        let errs = check("fn f(v) = match v {\n  { a, b } => 0\n  _ => 1\n}\n");
        assert!(
            errs.iter()
                .any(|e| e.message.contains("needs a record or `any` scrutinee")),
            "an unresolved scrutinee must not bind structural fields: {errs:?}"
        );
    }

    #[test]
    fn structural_fields_of_an_erased_scrutinee_are_erased_themselves() {
        // Over `any` the row is unknown until run time, so a bound field is
        // itself `any` — reading it with an operator is the erasure read the
        // checker deletes everywhere else.
        let errs = check(
            "fn f(v: any) -> int = match v {\n\
               { n } => n + 1\n\
               _ => 0\n\
             }\n",
        );
        assert!(
            errs.iter()
                .any(|e| e.message.contains("cannot apply `+` to an erased `any`")),
            "a narrowed field must stay erased until matched further: {errs:?}"
        );
    }

    #[test]
    fn a_match_over_any_requires_a_catch_all() {
        let errs = check(
            "fn f(v: any) -> int = match v {\n\
               { n } => 1\n\
             }\n",
        );
        assert!(
            errs.iter()
                .any(|e| e.message.contains("a match over `any` is never exhaustive")),
            "{errs:?}"
        );
    }

    #[test]
    fn an_open_row_arm_shadows_every_extension_after_it() {
        let errs = check(
            "fn f(v: any) -> int = match v {\n\
               { x, .. } => 1\n\
               { x, y } => 2\n\
               _ => 0\n\
             }\n",
        );
        assert!(
            errs.iter().any(|e| e
                .message
                .contains("`{ x, .. }` arm already covers this row")),
            "{errs:?}"
        );
        // Closed rows shadow only their exact repetition; reordering the
        // spelling does not make it a different row.
        let errs = check(
            "fn f(v: any) -> int = match v {\n\
               { x, y } => 1\n\
               { y, x } => 2\n\
               _ => 0\n\
             }\n",
        );
        assert!(
            errs.iter()
                .any(|e| e.message.contains("already matched by an earlier arm")),
            "{errs:?}"
        );
    }

    #[test]
    fn a_closed_structural_pattern_must_name_the_whole_row() {
        let errs = check(
            "type Point = { x: int, y: int }\n\
             fn f(p: Point) -> int = match p {\n\
               { x } => x\n\
             }\n",
        );
        assert!(
            errs.iter()
                .any(|e| e.message.contains("must name the whole row") && e.message.contains('y')),
            "{errs:?}"
        );
        // Opening the row with `..` selects the wider record.
        ok("type Point = { x: int, y: int }\n\
            fn f(p: Point) -> int = match p {\n\
              { x, .. } => x\n\
            }\n");
        // Naming a field the row lacks can never select.
        let errs = check(
            "type Point = { x: int, y: int }\n\
             fn f(p: Point) -> int = match p {\n\
               { z, .. } => z\n\
             }\n",
        );
        assert!(
            errs.iter().any(|e| e.message.contains("has no such field")),
            "{errs:?}"
        );
    }

    #[test]
    fn tuple_patterns_bind_positional_record_slots() {
        // `(n, s)` is the positional spelling of `{ 0, 1 }`; over a
        // positionally-declared record each binder takes its slot's declared
        // type ([PATTERN-TUPLE]).
        ok("type Pair = Pair(int, string)\n\
            fn f(p: Pair) -> int = match p {\n\
              (n, s) => n + length(s) ?: 0\n\
            }\n");
    }

    #[test]
    fn shape_reading_patterns_are_rejected_over_an_erased_scrutinee() {
        // Each of these arms reads the erased word with no runtime test — the
        // annotation-pattern spelling compiled before this rule and read a
        // heap address exactly as finding B did ([TYPE-ANY]).
        for (arm, kind) in [
            ("0 => 1", "a literal pattern"),
            ("s: string => 2", "a type-annotated binding"),
            ("[a, b] => 3", "a list pattern"),
        ] {
            let errs = check(&format!(
                "fn f(v: any) -> int = match v {{\n  {arm}\n  _ => 0\n}}\n"
            ));
            assert!(
                errs.iter()
                    .any(|e| e.message.contains(kind) && e.message.contains("cannot read")),
                "expected `{kind}` rejection for `{arm}`: {errs:?}"
            );
        }
        let errs = check(
            "type Color = Red | Green\n\
             fn f(v: any) -> int = match v {\n\
               Red => 1\n\
               _ => 0\n\
             }\n",
        );
        assert!(
            errs.iter().any(|e| e
                .message
                .contains("a variant pattern cannot read an erased `any`")),
            "{errs:?}"
        );
    }

    #[test]
    fn positional_constructor_destructures_fields() {
        ok("type Wrap = Wrap { value: int }\n\
            fn unwrap(w: Wrap) -> int = match w {\n\
              Wrap(v) => v\n\
            }\n");
        // A payload slot binds one level deep. A constructor nested inside it
        // has no fall-through spelling, so the Default flavor refuses it here
        // exactly as the ML parser refuses it ([FLAVOR-ML-PATTERN-GROUP]).
        let errs = check(
            "type Wrap = Wrap { value: int }\n\
            type Box = Box { item: Wrap }\n\
            fn deep(b: Box) -> int = match b {\n\
              Box(Wrap(v)) => v\n\
            }\n",
        );
        assert!(
            errs.iter()
                .any(|e| e.message.contains("nested constructor patterns")),
            "{errs:?}"
        );
    }

    #[test]
    fn record_constructor_pattern_unifies_owner() {
        // A `Ctor { fields }` pattern over a record type ties the discriminant to
        // the record owner type.
        ok("type Point = { x: int, y: int }\n\
            fn getx(p: Point) -> int = match p {\n\
              Point { x, y } => x\n\
            }\n");
    }

    #[test]
    fn bare_result_variant_bindings_match_the_builtin() {
        // `Success`/`Error` as bare bindings over a real Result match the builtin
        // variants and bind nothing.
        ok("fn truthy(r: Result<int, Error>) -> int = match r {\n\
              Success => 1\n\
              Error => 0\n\
            }\n");
    }

    #[test]
    fn nullary_union_variant_bindings_unify_and_are_exhaustive() {
        // No catch-all: each nullary binding is a variant, exercising both the
        // binding-unify path and exhaustiveness via `pattern_ctor_name`.
        ok("type Color = Red | Green | Blue\n\
            fn name(c: Color) -> string = match c {\n\
              Red => \"r\"\n\
              Green => \"g\"\n\
              Blue => \"b\"\n\
            }\n");
    }

    #[test]
    fn non_exhaustive_union_reports_missing_variants() {
        let errs = check(
            "type Color = Red | Green | Blue\n\
             fn name(c: Color) -> string = match c {\n\
               Red => \"r\"\n\
               Green => \"g\"\n\
             }\n",
        );
        assert!(errs
            .iter()
            .any(|e| e.message.contains("non-exhaustive") && e.message.contains("Blue")));
    }

    #[test]
    fn uppercase_binding_over_non_union_is_a_plain_binding() {
        // An uppercase name over an unconstrained (non-Con) discriminant is just a
        // catch-all binding — `unknown_variant_owner` returns None for non-Con.
        ok("fn f(v) = match v {\n\
              X => X\n\
            }\n");
        // An uppercase name that IS a real variant of the union is fine.
        ok("type Color = Red | Green\n\
            fn g(c: Color) -> int = match c {\n\
              Red => 1\n\
              Green => 2\n\
            }\n");
    }

    #[test]
    fn result_pattern_with_an_unknown_field_binds_a_fresh_var() {
        // `Success { surplus }` over a Result binds the standard `value`/`message`
        // and a fresh var for any other field name (the `_ => fresh()` arm).
        ok("fn f(r: Result<int, Error>) -> int = match r {\n\
              Success { value, surplus } => value\n\
              Error { message } => 0\n\
            }\n");
    }

    #[test]
    fn unknown_constructor_pattern_is_an_error_but_binds_fields() {
        let errs = check(
            "fn f(v) = match v {\n\
               Bogus { a, b } => a\n\
               _ => 0\n\
             }\n",
        );
        assert!(errs
            .iter()
            .any(|e| e.message.contains("unknown constructor `Bogus`")));
    }

    #[test]
    fn list_patterns_bind_head_tail_and_prefix() {
        ok("fn classify(xs) = match xs {\n\
              [] => \"empty\"\n\
              [single] => \"one\"\n\
              [head, ...tail] => \"many\"\n\
            }\n");
    }

    #[test]
    fn structural_pattern_over_a_record_value_binds_field_types() {
        // Matching a record *value* (not a nominal annotation) makes the
        // discriminant a real `Type::Record`, so structural binding reads the
        // field's declared type.
        ok("type Point = { x: int, y: int }\n\
            fn sum() -> int = match Point { x: 1, y: 2 } {\n\
              { x, y } => (x + y) ?: 0\n\
            }\n");
    }

    #[test]
    fn type_annotated_pattern_binds_the_named_type() {
        ok("fn f(v) = match v {\n\
              n: Int => n\n\
              _ => 0\n\
            }\n");
    }

    #[test]
    fn nullary_record_owner_pattern_unifies() {
        // `type Foo = Foo` is a nullary record constructor; matching it ties the
        // discriminant to the empty-record owner type (`nullary_owner_ty`'s
        // record arm).
        ok("type Foo = Foo\n\
            fn f(x: Foo) -> int = match x {\n\
              Foo => 1\n\
            }\n");
    }

    #[test]
    fn result_owner_constructor_pattern_autowraps_payload() {
        // A `Success { value }` pattern over a non-Result discriminant auto-wraps
        // it as the success payload, and `Error { message }` binds a string.
        ok("fn f(n: int) -> int = match n {\n\
              Success { value } => value\n\
              Error { message } => 0\n\
            }\n");
    }

    #[test]
    fn result_is_not_a_boolean_condition_and_bool_matches_stay_exhaustive() {
        let condition_errors = check("fn f(r: Result<int, Error>) -> int = r ? 1 : 0\n");
        assert!(condition_errors
            .iter()
            .any(|e| e.message.contains("Result") && e.message.contains("bool")));

        let errs = check("let x = match true { true => 1 }\n");
        assert!(errs.iter().any(|e| e.message.contains("non-exhaustive")));
    }

    #[test]
    fn bare_result_annotation_uses_the_no_args_unwrap_fallback() {
        // A bare `Result` annotation (no type args) is still a Result; matching a
        // A literal cannot directly match a Result wrapper.
        let errs = check(
            "fn f(r: Result) -> int = match r {\n\
               0 => 0\n\
               _ => 1\n\
             }\n",
        );
        assert!(errs.iter().any(|e| e.message.contains("Result")));
    }

    #[test]
    fn lowercase_binding_is_a_legal_catch_all() {
        // A genuine lower-case variable binding still absorbs the remaining
        // variants, keeping a partial union match legal.
        ok("type Color = Red | Green | Blue\n\
            fn name(c: Color) -> string = match c {\n\
              Red => \"r\"\n\
              other => \"?\"\n\
            }\n");
    }

    #[test]
    fn misspelled_uppercase_variant_is_not_a_catch_all() {
        // `Bleu` is not a `Color` variant: it must be reported rather than
        // silently absorbing the missing variants as a catch-all.
        let errs = check(
            "type Color = Red | Green | Blue\n\
             fn name(c: Color) -> string = match c {\n\
               Red => \"r\"\n\
               Bleu => \"?\"\n\
             }\n",
        );
        assert!(
            errs.iter().any(|e| e.message.contains("Bleu")),
            "expected an error naming the unknown variant `Bleu`: {errs:?}"
        );
    }

    #[test]
    fn arm_after_a_catch_all_is_unreachable() {
        let errs = check(
            "type Color = Red | Green | Blue\n\
             fn name(c: Color) -> string = match c {\n\
               Red => \"r\"\n\
               _ => \"?\"\n\
               Green => \"g\"\n\
             }\n",
        );
        assert!(
            errs.iter().any(|e| e.message.contains("unreachable")),
            "expected an unreachable-arm error: {errs:?}"
        );
    }

    #[test]
    fn duplicate_variant_arm_is_unreachable() {
        let errs = check(
            "type Color = Red | Green | Blue\n\
             fn name(c: Color) -> string = match c {\n\
               Red => \"r\"\n\
               Green => \"g\"\n\
               Red => \"r2\"\n\
               Blue => \"b\"\n\
             }\n",
        );
        assert!(
            errs.iter()
                .any(|e| e.message.contains("unreachable") && e.message.contains("Red")),
            "expected a duplicate-variant unreachable error: {errs:?}"
        );
    }
}
