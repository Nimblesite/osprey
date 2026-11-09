//! Exhaustiveness and redundancy of match arms.

use super::*;
use std::collections::HashSet;

impl Checker {
    /// Enforce exhaustiveness where it is unambiguous: `bool` needs both
    /// constructors; a known union/`Result` needs every variant — unless a
    /// catch-all arm is present.
    pub(super) fn check_exhaustive(&mut self, disc: &Type, arms: &[MatchArm]) {
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
    pub(super) fn check_redundant_arms(&mut self, arms: &[MatchArm], disc: &Type) {
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
    /// and names no variant.
    fn is_variable_binding(&self, name: &str) -> bool {
        !starts_uppercase(name) && !self.names_variant(name)
    }

    /// Whether a bare name selects a user variant, payload or not, as ML's
    /// upper-case patterns and codegen's `pattern_ctor` read it. The built-in
    /// `Result` variants keep their own auto-wrap rules. Implements
    /// [TYPE-MATCH-EXHAUSTIVE].
    pub(super) fn names_variant(&self, name: &str) -> bool {
        self.ctors
            .get(name)
            .is_some_and(|i| i.owner != names::RESULT)
    }

    /// The variant a pattern covers for exhaustiveness, if any: an explicit
    /// constructor, a bare built-in `Result` variant (`Success`/`Error`), or a
    /// bare user variant.
    fn pattern_ctor_name(&self, pattern: &Pattern) -> Option<String> {
        match pattern {
            Pattern::Constructor { name, .. } => Some(name.clone()),
            Pattern::Binding(name) if is_result_variant(name) || self.names_variant(name) => {
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
