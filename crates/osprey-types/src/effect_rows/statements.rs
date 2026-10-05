//! Effect row statements.
use super::{Analyzer, CallableEnv, Expr, NamedArgument, Stmt, Summary};

impl Analyzer<'_> {
    pub(super) fn statements(
        &self,
        statements: &[Stmt],
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Summary {
        let mut out = Summary::default();
        for statement in statements {
            // `let`, assignment and a bare expression all analyse their value
            // and flow its callable assignments; only the two binding forms go
            // on to re-point the name. The shadow mark lands AFTER the value is
            // analysed, so `let x = f(x)` still sees the outer `x`.
            let (name, value) = match statement {
                Stmt::Let { name, value, .. } | Stmt::Assignment { name, value, .. } => {
                    (Some(name), value)
                }
                Stmt::Expr { value, .. } => (None, value),
                _ => continue,
            };
            out.union(self.expression(value, scope, env));
            self.flow_callable_assignments(value, scope, env);
            if let Stmt::Let { name, .. } = statement {
                let _ = env.shadowed.insert(name.clone());
            }
            if let Some(name) = name {
                self.rebind_value(name, value, scope, env);
            }
        }
        out
    }

    /// Re-point `name` at the provenance of its new `value`, forgetting the old
    /// binding when the value carries none — the rebind `let` and assignment
    /// share.
    pub(super) fn rebind_value(
        &self,
        name: &str,
        value: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) {
        if let Some(provenance) = self.value(value, scope, env) {
            let _ = env.values.insert(name.to_owned(), provenance);
        } else {
            let _ = env.values.remove(name);
        }
    }

    /// The union of the rows every expression in `exprs` contributes. Every
    /// aggregate form performs this same fold — they differ only in what they
    /// hold their expressions in, so each passes its own projection.
    pub(super) fn union_over<'e>(
        &self,
        exprs: impl IntoIterator<Item = &'e Expr>,
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let mut out = Summary::default();
        for expr in exprs {
            out.union(self.expression(expr, scope, env));
        }
        out
    }

    pub(super) fn expressions(
        &self,
        expressions: &[Expr],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        self.union_over(expressions, scope, env)
    }

    pub(super) fn named_expressions(
        &self,
        arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        self.union_over(arguments.iter().map(|argument| &argument.value), scope, env)
    }
}
