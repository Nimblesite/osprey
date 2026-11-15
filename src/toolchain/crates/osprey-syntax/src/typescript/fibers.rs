//! `spawn`, `awaitFiber`, `yieldFiber`, `send` and `recv` are the existing
//! fiber expressions, not promises, and `unwrapOr(result, fallback)` is
//! Default `result ?: fallback`. Implements [FLAVOR-TS-INTEROP].
use super::context::{Lowerer, Result};
use osprey_ast::Expr;
use oxc_ast::ast::{ArrowFunctionBody, CallExpression, Expression};

const SPAWN: &str = "`spawn(work)` takes one function of no parameters";
const UNWRAP_OR: &str = "`unwrapOr(result, fallback)` takes a result and its fallback";

impl Lowerer<'_> {
    /// A fiber intrinsic, or `None` for an ordinary call. A local of the same
    /// name is an ordinary value.
    pub(super) fn fiber_intrinsic(&mut self, call: &CallExpression<'_>) -> Result<Option<Expr>> {
        let Expression::Identifier(callee) = &call.callee else {
            return Ok(None);
        };
        let name = callee.name.as_str();
        if self.bound_in_function(name) {
            return Ok(None);
        }
        let lowered = match name {
            "spawn" => self.spawn(call)?,
            "unwrapOr" => match self.arguments(&call.arguments)?.as_slice() {
                [result, fallback] => {
                    crate::desugar::result_default(result.clone(), fallback.clone())
                }
                _ => return Err(self.error(call.span, UNWRAP_OR)),
            },
            "awaitFiber" | "yieldFiber" | "send" | "recv" => {
                let arguments = self.arguments(&call.arguments)?;
                match (name, arguments.as_slice()) {
                    ("awaitFiber", [fiber]) => Expr::Await(Box::new(fiber.clone())),
                    ("yieldFiber", []) => Expr::Yield(None),
                    ("yieldFiber", [value]) => Expr::Yield(Some(Box::new(value.clone()))),
                    ("recv", [channel]) => Expr::Recv(Box::new(channel.clone())),
                    ("send", [channel, value]) => Expr::Send {
                        channel: Box::new(channel.clone()),
                        value: Box::new(value.clone()),
                    },
                    _ => {
                        return Err(self.error(
                            call.span,
                            format!("`{name}` takes the arguments of its fiber expression"),
                        ))
                    }
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(lowered))
    }

    /// `spawn(() => work(x))` spawns `work(x)`; `spawn(work)` spawns `work()`.
    fn spawn(&mut self, call: &CallExpression<'_>) -> Result<Expr> {
        let [argument] = call.arguments.as_slice() else {
            return Err(self.error(call.span, SPAWN));
        };
        let work = match argument.as_expression() {
            Some(Expression::ArrowFunctionExpression(arrow)) => {
                let body = match &arrow.body {
                    ArrowFunctionBody::FunctionBody(_) => None,
                    other => other.as_expression(),
                };
                match (arrow.params.items.is_empty() && !arrow.r#async, body) {
                    (true, Some(body)) => self.expr(body)?,
                    (true, None) => apply_lambda(self.arrow(arrow)?),
                    (false, _) => return Err(self.error(call.span, SPAWN)),
                }
            }
            Some(work) => apply_lambda(self.expr(work)?),
            None => return Err(self.error(call.span, SPAWN)),
        };
        Ok(Expr::Spawn(Box::new(work)))
    }
}

fn apply_lambda(work: Expr) -> Expr {
    Expr::Call {
        function: Box::new(work),
        arguments: Vec::new(),
        named_arguments: Vec::new(),
    }
}
