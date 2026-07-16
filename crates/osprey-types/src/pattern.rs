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
use std::collections::{BTreeMap, HashMap, HashSet};

fn unwrap_result(t: &Type) -> Type {
    match t {
        Type::Con { name, args } if name == names::RESULT => match args.first() {
            Some(first) => first.clone(),
            None => t.clone(),
        },
        _ => t.clone(),
    }
}

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

/// Whether every arm is a `true`/`false` literal pattern — the shape the
/// ternary/Elvis desugar produces.
fn all_bool_literal_arms(arms: &[MatchArm]) -> bool {
    !arms.is_empty()
        && arms.iter().all(
            |a| matches!(&a.pattern, Pattern::Literal(e) if matches!(e.as_ref(), Expr::Bool(_))),
        )
}

impl Checker {
    pub(crate) fn infer_match(&mut self, value: &Expr, arms: &[MatchArm], env: &TypeEnv) -> Type {
        let disc = self.infer_expr(value, env);
        // Truthiness test: `true`/`false` arms over a `Result` (the ternary and
        // Elvis desugar) test the discriminant, the arms cover Success/Error,
        // and the match yields the *unwrapped* success payload.
        let truthy = is_result(&self.ctx.prune(&disc)) && all_bool_literal_arms(arms);
        let result = self.ctx.fresh();
        for arm in arms {
            let body_ty = self.infer_arm(arm, &disc, truthy, env);
            self.push_unify(&result, &body_ty);
        }
        if truthy {
            self.check_bool_exhaustive(arms);
        } else {
            self.check_exhaustive(&disc, arms);
        }
        self.check_redundant_arms(arms);
        result
    }

    /// Infer one arm's body type — binding its pattern against the discriminant
    /// unless this is a truthiness match, whose arms bind nothing and merge as
    /// the unwrapped success payload.
    fn infer_arm(&mut self, arm: &MatchArm, disc: &Type, truthy: bool, env: &TypeEnv) -> Type {
        let mut local = env.child();
        if !truthy {
            self.bind_pattern(&arm.pattern, disc, &mut local);
        }
        let body_ty = self.infer_expr(&arm.body, &local);
        if truthy {
            unwrap_result(&self.ctx.prune(&body_ty))
        } else {
            body_ty
        }
    }

    /// `select { pattern => body ... }` — same arm-typing as match without a
    /// concrete discriminant.
    pub(crate) fn infer_arm_bodies(&mut self, arms: &[MatchArm], env: &TypeEnv) -> Type {
        let result = self.ctx.fresh();
        for arm in arms {
            let mut local = env.child();
            let disc = self.ctx.fresh();
            self.bind_pattern(&arm.pattern, &disc, &mut local);
            let body_ty = self.infer_expr(&arm.body, &local);
            self.push_unify(&result, &body_ty);
        }
        result
    }

    fn bind_pattern(&mut self, pattern: &Pattern, disc: &Type, local: &mut TypeEnv) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Binding(name) => self.bind_binding(name, disc, local),
            Pattern::Literal(expr) => {
                let lt = self.infer_expr(expr, local);
                let du = unwrap_result(&self.ctx.prune(disc));
                self.push_unify(&du, &lt);
            }
            Pattern::TypeAnnotated { name, ty } => {
                let t = type_expr_to_type(ty, &HashMap::new());
                local.insert(name.clone(), Scheme::mono(t));
            }
            Pattern::Structural { fields } => {
                let dp = self.ctx.prune(disc);
                for fname in fields {
                    let ft = match &dp {
                        Type::Record { fields: rf, .. } => {
                            rf.get(fname).cloned().unwrap_or_else(|| self.ctx.fresh())
                        }
                        _ => self.ctx.fresh(),
                    };
                    local.insert(fname.clone(), Scheme::mono(ft));
                }
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
            let field_ty = declared.get(i).map(|(_, t)| t.clone());
            let target = field_ty.unwrap_or_else(|| self.ctx.fresh());
            self.bind_pattern(sub, &target, local);
        }
    }

    /// Bind the built-in `Result` pattern fields against `disc`: `value` is the
    /// success payload (the discriminant itself when it is not a `Result` — the
    /// match auto-wrap rule), `message` is the error string.
    fn bind_result_fields(&mut self, fields: &[String], disc: &Type, local: &mut TypeEnv) {
        let dp = self.ctx.prune(disc);
        let ok = match &dp {
            Type::Con { name, args } if name == names::RESULT && !args.is_empty() => {
                args.first().cloned().unwrap_or_else(|| dp.clone())
            }
            _ => dp.clone(),
        };
        for fname in fields {
            let ft = match fname.as_str() {
                "value" => ok.clone(),
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
    /// arm, and a repeated constructor/variant arm. A catch-all stays legal — it
    /// suppresses the missing-variant error — but dead arms after it (or duplicate
    /// variants) are genuine mistakes, so report them.
    fn check_redundant_arms(&mut self, arms: &[MatchArm]) {
        let mut covered_all = false;
        let mut seen: HashSet<String> = HashSet::new();
        for arm in arms {
            if covered_all {
                self.errors.push(TypeError::new(
                    "unreachable match arm: an earlier catch-all already covers every case",
                ));
                continue;
            }
            if let Some(name) = self.pattern_ctor_name(&arm.pattern) {
                if !seen.insert(name.clone()) {
                    self.errors.push(TypeError::new(format!(
                        "unreachable match arm: variant `{name}` is already matched by an earlier arm"
                    )));
                }
            }
            covered_all = self.is_irrefutable(&arm.pattern);
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
              { x, y } => x + y\n\
            }\n");
        // A structural pattern over a non-record discriminant binds fresh vars.
        ok("fn any(v) = match v {\n\
              { a, b } => 0\n\
            }\n");
    }

    #[test]
    fn positional_constructor_destructures_fields() {
        ok("type Wrap = Wrap { value: int }\n\
            fn unwrap(w: Wrap) -> int = match w {\n\
              Wrap(v) => v\n\
            }\n");
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
              { x, y } => x + y\n\
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
    fn bool_truthy_match_needs_both_branches() {
        // A `true`/`false` (truthiness) match missing a branch is non-exhaustive.
        let errs = check("let r = (10 + 5) ? 1 : 2\nlet x = match r > 0 { true => 1 }\n");
        assert!(errs.iter().any(|e| e.message.contains("non-exhaustive")));
    }

    #[test]
    fn bare_result_annotation_uses_the_no_args_unwrap_fallback() {
        // A bare `Result` annotation (no type args) is still a Result; matching a
        // literal against it runs `unwrap_result`'s no-args fallback (the empty
        // Result cannot equal the int literal, so a mismatch is reported — the
        // point is that the fallback branch is executed without panicking).
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
