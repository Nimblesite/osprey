//! Type checking: functions.
use super::{
    BTreeSet, Checker, EffectRef, Expr, HashMap, Parameter, Position, Program, Scheme, Stmt, Type,
    TypeEnv,
};

impl Checker {
    /// Pass two: infer bodies and run top-level statements.
    pub(super) fn check(&mut self, program: &Program, env: &mut TypeEnv) {
        self.check_statements(&mut program.statements.iter(), env);
    }

    pub(super) fn check_statements(
        &mut self,
        statements: &mut dyn Iterator<Item = &Stmt>,
        env: &mut TypeEnv,
    ) {
        for stmt in statements {
            match stmt {
                Stmt::Function {
                    name,
                    parameters,
                    effects,
                    body,
                    position,
                    ..
                } => {
                    let outer = std::mem::replace(&mut self.site, name.clone());
                    let outer_site = self.ctx.enter(name.clone());
                    self.check_function(name, parameters, effects, body, env, *position);
                    let _ = self.ctx.enter(outer_site);
                    self.site = outer;
                }
                Stmt::Module { body, .. } => {
                    let mut inner = env.child();
                    // Module declarations live in their own lexical scope. Run
                    // both checker passes there: the old implementation only
                    // ran pass two, so module functions were never registered
                    // and their bodies were silently skipped.
                    self.collect_statements(
                        body.iter().map(|item| item.declaration.as_ref()),
                        &mut inner,
                    );
                    self.check_statements(
                        &mut body.iter().map(|item| item.declaration.as_ref()),
                        &mut inner,
                    );
                }
                Stmt::Namespace { body, .. } => {
                    let mut inner = env.child();
                    self.collect_statements(body.iter(), &mut inner);
                    self.check_statements(&mut body.iter(), &mut inner);
                }
                // `let` / assignment / bare-expr statements infer the same way at
                // top level and inside a block.
                other => self.infer_block_stmt(other, env),
            }
        }
    }

    pub(super) fn check_function(
        &mut self,
        name: &str,
        parameters: &[Parameter],
        effects: &[EffectRef],
        body: &Expr,
        env: &mut TypeEnv,
        pos: Option<Position>,
    ) {
        let (params, ret) = match self.fn_sigs.get(name) {
            Some(sig) => sig.clone(),
            None => return,
        };
        let mut local = env.child();
        for (p, ty) in parameters.iter().zip(&params) {
            local.insert(p.name.clone(), Scheme::mono(ty.clone()));
        }
        // The declared effect row instantiates each referenced effect for the
        // body's `perform` sites (`!State<int>` pins `T` to `int`).
        // Implements [EFFECTS-GENERIC-ROWS].
        let typarams = self.fn_typarams.get(name).cloned().unwrap_or_default();
        let mut scopes = Vec::new();
        let mut declared_rows = Vec::new();
        for effect in effects {
            let scope = self.effect_row_scope(effect, &typarams, pos);
            declared_rows.push(
                (!effect.type_args.is_empty())
                    .then(|| scope.as_ref().map(|resolved| resolved.args.clone()))
                    .flatten(),
            );
            if let Some(scope) = scope {
                scopes.push(scope);
            }
        }
        if let Some(position) = pos {
            let _ = self
                .declared_effect_rows
                .insert((position.line, position.column), declared_rows);
        }
        let pushed = scopes.len();
        self.handler_scopes.extend(scopes);
        self.current_fn_typarams = typarams;
        let body_ty = self.infer_expr(body, &local);
        self.current_fn_typarams = HashMap::new();
        self.handler_scopes
            .truncate(self.handler_scopes.len().saturating_sub(pushed));
        self.unify_or_err(&ret, &body_ty, &format!("function `{name}` body"), pos);
        // Generalize the now-constrained signature so later call sites can use
        // the function polymorphically (HM let-generalization for top-level fns).
        // Remove the function's own monomorphic entry first, else its signature
        // variables would count as "free in the environment" and nothing would
        // generalize.
        let fun_ty = Type::fun(params, ret);
        let declared = env
            .applied(&mut self.ctx, name)
            .map(|applied| applied.params)
            .unwrap_or_default();
        env.remove(name);
        let mut scheme = self.generalize_with_obligations(env, &fun_ty);
        // Explicit binders are quantified even when the signature never uses
        // them. Two applications of a phantom binder are still independent.
        let env_vars = env.free_vars(&mut self.ctx);
        for param in &declared {
            let mut vars = BTreeSet::new();
            self.ctx.free_vars(param, &mut vars);
            for var in vars.difference(&env_vars) {
                if !scheme.vars.contains(var) {
                    scheme.vars.push(*var);
                }
            }
        }
        let _ = self
            .scheme_obligations
            .insert(format!("fn {name}"), scheme.obligations.clone());
        env.insert(name, scheme);
        env.declare_type_params(name, declared);
    }
}
