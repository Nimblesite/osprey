//! Binary operators: deferred arithmetic overloads, numeric constraints,
//! comparison and negation.

use super::*;

/// Marks a pending arithmetic overload in the obligation list
/// ([`Checker::deferred_arith`]). Obligations already carry per-site types
/// through generalization and instantiation, and an operator can never collide
/// with a built-in's name, so a deferred overload rides that list rather than a
/// parallel one.
const DEFERRED_ARITH: &str = "arith ";

fn deferred_arith_name(op: &str) -> String {
    format!("{DEFERRED_ARITH}{op}")
}

/// The operator a deferred-arithmetic obligation name carries; `None` for a
/// built-in's obligation.
fn parse_deferred_arith(name: &str) -> Option<&str> {
    if !name.starts_with(DEFERRED_ARITH) {
        return None;
    }
    crate::builtin_constraints::operation_name(name).strip_prefix(DEFERRED_ARITH)
}

/// Whether an obligation marks a pending arithmetic overload rather than a
/// built-in's representation constraint.
pub(crate) fn is_deferred_arith(name: &str) -> bool {
    name.starts_with(DEFERRED_ARITH)
}

/// Division has a result relation, not one shared overload: integer operands
/// still yield float, and each sized-float instantiation yields its own width.
pub(crate) fn is_monomorphic_arith(name: &str) -> bool {
    parse_deferred_arith(name).is_some_and(|op| op != "/")
}

fn is_result(t: &Type) -> bool {
    t.is_named(names::RESULT)
}

impl Checker {
    /// Reject an operation that would consume an erased `any` word directly.
    /// Every operator reads its operand's representation, and an erased word
    /// has none to read — comparing one worked only while a scalar's erasure
    /// happened to be the identity, and compared heap addresses otherwise
    /// ([TYPE-ANY]). Reports the error and returns whether it fired.
    pub(super) fn reject_erased_operand(&mut self, what: &str, ty: &Type) -> bool {
        let erased = self.ctx.expose(ty).is_named(names::ANY);
        if erased {
            self.errors.push(TypeError::new(format!(
                "cannot {what} an erased `any`: match its structure instead"
            )));
        }
        erased
    }

    pub(super) fn infer_binary(
        &mut self,
        op: &str,
        left: &Expr,
        right: &Expr,
        position: Option<osprey_ast::Position>,
        env: &TypeEnv,
    ) -> Type {
        let lt = self.infer_expr(left, env);
        let rt = self.infer_expr(right, env);
        let what = format!("apply `{op}` to");
        if self.reject_erased_operand(&what, &lt) || self.reject_erased_operand(&what, &rt) {
            return match classify(op) {
                OpKind::Logical | OpKind::Comparison => Type::bool(),
                OpKind::Arith => Type::int(),
            };
        }
        match classify(op) {
            OpKind::Logical => {
                self.push_assign(&Type::bool(), &lt);
                self.push_assign(&Type::bool(), &rt);
                Type::bool()
            }
            OpKind::Comparison => {
                let l = self.ctx.expose(&lt);
                let r = self.ctx.expose(&rt);
                if is_result(&l) || is_result(&r) {
                    self.errors.push(TypeError::new(
                        "cannot compare a `Result` directly; handle it explicitly with `match` or `?:`",
                    ));
                } else if unify(&mut self.ctx, &l, &r).is_err() {
                    // Both operands lower to one machine comparison, so two
                    // unrelated types would compare representations: `true < 1`
                    // answered, and an opaque alias answered to an integer.
                    // Implements [BOOL-COMPARE-OPERANDS].
                    let (l, r) = (self.ctx.displayed(&l), self.ctx.displayed(&r));
                    self.errors.push(TypeError::new(format!(
                        "cannot compare `{l}` with `{r}`: `{op}` needs two operands of one type"
                    )));
                } else {
                    self.constrain_comparison(op, [left, right], &l, position);
                }
                Type::bool()
            }
            OpKind::Arith => self.infer_arith(op, &lt, &rt, position),
        }
    }

    fn infer_arith(
        &mut self,
        op: &str,
        lt: &Type,
        rt: &Type,
        position: Option<osprey_ast::Position>,
    ) -> Type {
        let l = self.ctx.expose(lt);
        let r = self.ctx.expose(rt);
        // [TYPE-NUMERIC-ARITH-WIDTH] Division's result follows its operands,
        // except integer division, whose result remains binary64.
        if op == "/" && self.defers(&l, &r) {
            let result = self.ctx.fresh();
            let name = self.located_use(&deferred_arith_name(op), position);
            self.builtin_uses
                .push((name, Type::fun(vec![l, r], result.clone())));
            return result;
        }
        let string_concat = op == "+" && (l.is_named(names::STRING) || r.is_named(names::STRING));
        if !string_concat {
            if let Some(result) = self.sized_arith(op, &l, &r) {
                return result;
            }
        }
        // [TYPE-NUMERIC-LITERAL] With no sized sibling, an open float literal
        // settles before the existing float overload and int promotion run.
        let l = self.settle_arith_float(&l);
        let r = self.settle_arith_float(&r);
        if !string_concat && (op == "/" || l.is_named(names::FLOAT) || r.is_named(names::FLOAT)) {
            self.constrain_numeric_operands(op, &l, &r, position);
        }
        match op {
            "%" if l.is_named(names::FLOAT) || r.is_named(names::FLOAT) => Type::float(),
            "%" if self.defers(&l, &r) => self.deferred_arith(op, &l, &r, position),
            "%" => self.int_arithmetic_result(&l, &r),
            "/" => Type::float(),
            "+" => {
                if string_concat {
                    self.push_unify(&Type::string(), &l);
                    self.push_unify(&Type::string(), &r);
                    Type::string()
                } else if l.is_named(names::FLOAT) || r.is_named(names::FLOAT) {
                    Type::float()
                } else if l.is_named(names::LIST) {
                    self.push_unify(&l, &r);
                    l
                } else if r.is_named(names::LIST) {
                    self.push_unify(&l, &r);
                    r
                } else if l.is_named(names::MAP) || r.is_named(names::MAP) {
                    self.push_unify(&l, &r);
                    if l.is_named(names::MAP) {
                        l
                    } else {
                        r
                    }
                } else if self.defers(&l, &r) {
                    self.deferred_arith(op, &l, &r, position)
                } else {
                    self.int_arithmetic_result(&l, &r)
                }
            }
            // Unlike `+`, `-` and `*` have no string/list overload; their
            // unconstrained form still defers, and resolves without the string
            // case ([`Checker::resolve_deferred_arith`]).
            _ => {
                if self.defers(&l, &r) {
                    self.deferred_arith(op, &l, &r, position)
                } else if l.is_named(names::FLOAT) || r.is_named(names::FLOAT) {
                    Type::float()
                } else {
                    self.int_arithmetic_result(&l, &r)
                }
            }
        }
    }

    fn settle_arith_float(&mut self, operand: &Type) -> Type {
        if self.ctx.is_open_float_literal(operand) {
            self.ctx.expose_settled(operand)
        } else {
            self.ctx.expose(operand)
        }
    }

    /// [FLOAT-OPERANDS] Keep the numeric requirement on each scheme variable;
    /// unifying with float here would wrongly reject valid integer callers.
    fn constrain_numeric_operands(
        &mut self,
        op: &str,
        left: &Type,
        right: &Type,
        position: Option<osprey_ast::Position>,
    ) {
        use crate::builtin_constraints::{is_numeric_scalar, NUMERIC_OPERAND_PREFIX};
        for ty in [left, right] {
            if !is_numeric_scalar(ty) {
                let name = self.located_use(&format!("{NUMERIC_OPERAND_PREFIX}{op}"), position);
                self.builtin_uses.push((name, ty.clone()));
            }
        }
    }

    /// Whether an arithmetic site with these (unwrapped) operands may still
    /// leave its overload open. Two unconstrained variables can; and only while
    /// inference is still running — the resolution pass re-enters
    /// [`Self::infer_arith`] with deferral CLOSED, so a site nothing ever
    /// constrained lands on the integer default there.
    fn defers(&mut self, left: &Type, right: &Type) -> bool {
        self.defer_arith
            && both_vars(left, right)
            && !self.ctx.is_open_literal(left)
            && !self.ctx.is_open_literal(right)
    }

    /// Type an arithmetic operator whose BOTH operands are still unconstrained
    /// variables, without committing to the integer overload.
    ///
    /// Eagerly defaulting here is what made a named numeric helper unusable as
    /// a float kernel: `fn plus(a, x) = a + x` is checked before anything says
    /// what `a` is, so it became `(int, int) -> int` and
    /// `gpuFold(0.0, plus)` over a float buffer was rejected with `cannot unify
    /// int with float` ([GPU-KERNEL-ELEM-TYPING], [GPU-KERNEL-FORM]). A lambda
    /// in the same slot already works, because its parameters are pinned from
    /// the slot before its body is inferred ([`Self::positional_arg_types`]);
    /// this is the same rule for the named form.
    ///
    /// Both operands unify with one another and the result is left OPEN. The
    /// overload is picked once, after all unification, by re-running the very
    /// same selection over the operands' final types
    /// ([`Self::resolve_deferred_arith`]) — so the pending site records the
    /// operands AS WRITTEN, error channel included, and not the unwrapped pair
    /// the selection happens to compare.
    ///
    /// An operand carrying a pending overload does NOT generalize
    /// ([`Checker::generalize_with_obligations`]): with no numeric class to
    /// quantify over, one definition gets ONE overload, chosen by how the
    /// program actually uses it. `fn plus(a, x) = a + x` folded over a float
    /// buffer is a float addition; the same helper used only on integers is
    /// still the checked integer one; using it at both in a single program is a
    /// type error rather than a silent reinterpretation.
    fn deferred_arith(
        &mut self,
        op: &str,
        l: &Type,
        r: &Type,
        position: Option<osprey_ast::Position>,
    ) -> Type {
        self.push_unify(l, r);
        let result = self.ctx.fresh();
        let name = self.located_use(&deferred_arith_name(op), position);
        self.builtin_uses
            .push((name, Type::fun(vec![l.clone(), r.clone()], result.clone())));
        result
    }

    /// The key of an arithmetic obligation written here. Where the function
    /// reads through an opaque alias, the key remembers that function
    /// ([`Checker::use_sites`]).
    pub(crate) fn located_use(
        &mut self,
        name: &str,
        position: Option<osprey_ast::Position>,
    ) -> String {
        let located = crate::builtin_constraints::located_name(name, position);
        if self.ctx.sees_through_aliases() {
            let _ = self.use_sites.insert(located.clone(), self.site.clone());
        }
        located
    }

    /// A scheme's built-in obligations restated for the use reading them now:
    /// a key that already carries a location keeps it (it was located where
    /// the use was written, inside the wrapper's body), and a structural key
    /// — a field or method relation, which encodes its own site — keeps its
    /// spelling. A bare one — `abs`'s scheme obligations, a constrained
    /// built-in — is located at this use, so its diagnostics name the
    /// expression that triggered them.
    pub(super) fn located_obligations(
        &mut self,
        obligations: Vec<(String, Type)>,
        position: Option<osprey_ast::Position>,
    ) -> Vec<(String, Type)> {
        obligations
            .into_iter()
            .map(|(name, ty)| {
                if locatable_obligation(&name) {
                    (self.located_use(&name, position), ty)
                } else {
                    (name, ty)
                }
            })
            .collect()
    }

    /// The key of a representation-sensitive use with no position of its own.
    /// Inside a module that owns an opaque alias the key names the function, so
    /// the late validation reads the operand as that function may
    /// ([MODULES-OPAQUE-TYPES]).
    pub(crate) fn sited_use(&mut self, name: &str) -> String {
        if !self.ctx.sees_through_aliases() {
            return name.to_owned();
        }
        let sited = format!("{name} @{}", self.site);
        let _ = self.use_sites.insert(sited.clone(), self.site.clone());
        sited
    }

    /// Settle one deferred site: re-run the ordinary overload selection over
    /// the operands' final types with deferral closed, and tie the site's open
    /// result to the answer. Re-running rather than restating the rules is what
    /// keeps `p.x * p.x + p.y * p.y` right — by the time the outer `+` resolves,
    /// its operands have become `int`, and the ordinary selection seeds each
    /// fallible operator's Arith requirement. A result another use already
    /// pinned decides still-open operands first: `(a - b) <= 1.0` unified the
    /// result with float, so the operands are float, not the integer default.
    pub(crate) fn resolve_deferred_arith(&mut self, name: &str, site: &Type) {
        let Some(op) = parse_deferred_arith(name) else {
            return;
        };
        let Type::Fun { params, ret } = self.ctx.apply(site) else {
            return;
        };
        let position = crate::builtin_constraints::source_position(name);
        if crate::arithmetic::is_float_open(op) {
            for operand in &params {
                if matches!(self.ctx.apply(operand), Type::Var(_)) {
                    self.push_unify(&Type::float(), operand);
                }
            }
        }
        // The operator reads its operands where it was written, and its
        // diagnostics say so too.
        let written = self.use_sites.get(name).cloned().unwrap_or_default();
        let outer = self.ctx.enter(written);
        let outer_position = self.expr_position;
        self.expr_position = position;
        if op != "/" && !matches!(self.ctx.apply(&ret), Type::Var(_)) {
            for operand in &params {
                if matches!(self.ctx.apply(operand), Type::Var(_)) {
                    self.push_unify(&ret, operand);
                }
            }
        }
        let uses_before = self.builtin_uses.len();
        let answer = match params.as_slice() {
            [operand] if crate::arithmetic::absolute_operand(operand) => {
                Some(self.infer_negation(operand))
            }
            [left, right] => Some(self.infer_arith(op, left, right, position)),
            _ => None,
        };
        if op == "/" {
            self.place_arithmetic_uses(name, site, uses_before);
        }
        if let Some(answer) = answer {
            self.push_unify(&answer, &ret);
        }
        self.expr_position = outer_position;
        let _ = self.ctx.enter(outer);
    }

    /// Instantiation may decide division late; its diagnostics retain the
    /// obligation's original position in the validation sequence.
    fn place_arithmetic_uses(&mut self, name: &str, site: &Type, start: usize) {
        self.field_uses_prefix = None;
        let introduced = self.builtin_uses.split_off(start);
        let at = self
            .builtin_uses
            .iter()
            .position(|(pending, pending_site)| pending == name && pending_site == site)
            .map_or(start, |index| index + 1);
        drop(self.builtin_uses.splice(at..at, introduced));
    }

    /// [TYPE-NUMERIC-SAME-TYPE] An operator with a sized operand takes both at
    /// that one type, so `int32 + int` and `float32 + float` do not unify; the
    /// `int`→`float` promotion does not reach a sized type. `/` over integers
    /// is `float` at every width; `None` when neither operand is sized.
    fn sized_arith(&mut self, op: &str, left: &Type, right: &Type) -> Option<Type> {
        let (ty, numeric) = [left, right].into_iter().find_map(|ty| {
            Numeric::of_type(ty)
                .filter(|n| n.is_sized())
                .map(|n| (ty.clone(), n))
        })?;
        self.push_unify(left, right);
        Some(if op == "/" && numeric.is_integer() {
            Type::float()
        } else {
            ty
        })
    }

    /// Constrain both operands to integers. An out-of-range result performs
    /// `Arith.overflow` instead ([ARITH-EFFECT]). Two open literals stay open
    /// together, so `let x: uint8 = 1 + 2` is `uint8` ([TYPE-NUMERIC-LITERAL]).
    fn int_arithmetic_result(&mut self, left: &Type, right: &Type) -> Type {
        if self.ctx.is_open_literal(left) && self.ctx.is_open_literal(right) {
            self.push_unify(left, right);
            return left.clone();
        }
        self.push_unify(&Type::int(), left);
        self.push_unify(&Type::int(), right);
        Type::int()
    }

    /// Negation keeps its operand's numeric type; float negation is total and
    /// integer negation performs `Arith.overflow` where the result does not
    /// fit ([ARITH-EFFECT], [TYPE-NUMERIC-ARITH-WIDTH]).
    pub(super) fn infer_negation(&mut self, operand: &Type) -> Type {
        let inner = self.ctx.expose(operand);
        if Numeric::of_type(&inner).is_some() || self.ctx.is_open_literal(&inner) {
            inner
        } else {
            self.push_unify(&Type::int(), &inner);
            Type::int()
        }
    }
}

enum OpKind {
    Arith,
    Comparison,
    Logical,
}

fn classify(op: &str) -> OpKind {
    match op {
        "&&" | "||" => OpKind::Logical,
        "==" | "!=" | "<" | "<=" | ">" | ">=" => OpKind::Comparison,
        _ => OpKind::Arith,
    }
}
