//! `perform<E>().op(…)`, `handler<E>(arms)`, `handle<E>(arms, () => body)`
//! and `resume(value)` lower to the shared effects core, and a handler arm's
//! assignment to an enclosing `let` is handler-owned state.
//! Implements [FLAVOR-TS-EFFECTS] and [EFFECTS-HANDLER-STATE].
use super::context::{Lowerer, Result};
use osprey_ast::{Clauses, Expr, HandlerArm, Stage};
use oxc_ast::ast::{
    ArrowFunctionBody, ArrowFunctionExpression, CallExpression, Expression, ObjectExpression,
    ObjectPropertyKind, TSType, TSTypeParameterInstantiation,
};
use oxc_span::{GetSpan, Span};

/// The arms of a handler object, and its `return` and `finally` clauses.
struct Arms {
    arms: Vec<HandlerArm>,
    clauses: Clauses,
}

impl Lowerer<'_> {
    /// An effect intrinsic, or `None` for an ordinary call. A local of the
    /// same name is an ordinary value.
    pub(super) fn effect_intrinsic(&mut self, call: &CallExpression<'_>) -> Result<Option<Expr>> {
        match &call.callee {
            Expression::StaticMemberExpression(member) => {
                let Expression::CallExpression(request) = &member.object else {
                    return Ok(None);
                };
                if !self.ambient(&request.callee, "perform") {
                    return Ok(None);
                }
                Ok(Some(Expr::Perform {
                    effect: self.named_effect(
                        request.type_arguments.as_deref(),
                        request.span,
                        PERFORM,
                    )?,
                    operation: member.property.name.as_str().to_owned(),
                    arguments: self.arguments(&call.arguments)?,
                    named_arguments: Vec::new(),
                    position: Some(self.at(call.span)),
                }))
            }
            callee if self.ambient(callee, "perform") => Err(self.error(
                call.span,
                "`perform<E>()` must be called as an operation: `perform<E>().op(…)`",
            )),
            callee if self.ambient(callee, "resume") => self.resume(call).map(Some),
            callee if self.ambient(callee, "handler") => {
                self.handler(call, Stage::Dynamic).map(Some)
            }
            callee if self.ambient(callee, "staticHandler") => {
                self.handler(call, Stage::Static).map(Some)
            }
            callee if self.ambient(callee, "handle") => self.handle(call, Stage::Dynamic).map(Some),
            callee if self.ambient(callee, "handleStatic") => {
                self.handle(call, Stage::Static).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Whether `callee` is the ambient name `name`, not a local.
    fn ambient(&self, callee: &Expression<'_>, name: &str) -> bool {
        matches!(callee, Expression::Identifier(id) if id.name.as_str() == name && !self.bound_in_function(name))
    }

    /// The effect `E` of `perform<E>`, `handler<E>` or `handle<E>`.
    fn named_effect(
        &self,
        arguments: Option<&TSTypeParameterInstantiation<'_>>,
        span: Span,
        usage: &str,
    ) -> Result<String> {
        let Some([TSType::TSTypeReference(reference)]) =
            arguments.map(|list| list.params.as_slice())
        else {
            return Err(self.error(span, usage));
        };
        let base = self.type_name(&reference.type_name)?;
        let arguments = self.type_arguments(reference.type_arguments.as_deref())?;
        Ok(osprey_ast::effect_name::instantiated(&base, &arguments))
    }

    fn resume(&mut self, call: &CallExpression<'_>) -> Result<Expr> {
        let mut arguments = self.arguments(&call.arguments)?;
        if arguments.len() > 1 {
            return Err(self.error(call.span, "`resume` takes at most one value"));
        }
        Ok(Expr::Resume(arguments.pop().map(Box::new)))
    }

    /// `handler<E>({ op: (x) => … })` is a callable handler value.
    fn handler(&mut self, call: &CallExpression<'_>, stage: Stage) -> Result<Expr> {
        let effect = self.named_effect(call.type_arguments.as_deref(), call.span, HANDLER)?;
        let [argument] = call.arguments.as_slice() else {
            return Err(self.error(call.span, HANDLER));
        };
        let Some(Expression::ObjectExpression(object)) = argument.as_expression() else {
            return Err(self.error(argument.span(), "a handler takes an object literal of arms"));
        };
        let Arms { arms, clauses } = self.handler_arms(object)?;
        let position = Some(self.at(call.span));
        Ok(osprey_ast::handler_value(
            stage, effect, arms, clauses, position,
        ))
    }

    /// `handle<E>(arms, () => body)` installs the arms around `body`, which is
    /// lowered in the enclosing function: its `let`s are that function's.
    fn handle(&mut self, call: &CallExpression<'_>, stage: Stage) -> Result<Expr> {
        let effect = self.named_effect(call.type_arguments.as_deref(), call.span, HANDLE)?;
        let [arms, body] = call.arguments.as_slice() else {
            return Err(self.error(call.span, HANDLE));
        };
        let (
            Some(Expression::ObjectExpression(object)),
            Some(Expression::ArrowFunctionExpression(body)),
        ) = (arms.as_expression(), body.as_expression())
        else {
            return Err(self.error(call.span, HANDLE));
        };
        if !body.params.items.is_empty() || body.r#async {
            return Err(self.error(body.span, HANDLE));
        }
        let Arms { arms, clauses } = self.handler_arms(object)?;
        let body = self.scoped(false, true, |lowerer| lowerer.inline_body(body))?;
        Ok(Expr::Handler {
            stage,
            effect,
            arms,
            return_clause: clauses.return_clause,
            finally: clauses.finally,
            body: Box::new(body),
            position: Some(self.at(call.span)),
        })
    }

    fn inline_body(&mut self, arrow: &ArrowFunctionExpression<'_>) -> Result<Expr> {
        match &arrow.body {
            ArrowFunctionBody::FunctionBody(body) => {
                self.block(&body.statements.iter().collect::<Vec<_>>(), body.span)
            }
            other => match other.as_expression() {
                Some(expression) => self.expr(expression),
                None => self.unsupported(arrow.span, "arrow function body"),
            },
        }
    }

    fn handler_arms(&mut self, object: &ObjectExpression<'_>) -> Result<Arms> {
        let mut lowered = Arms {
            arms: Vec::new(),
            clauses: Clauses::default(),
        };
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return Err(self.error(property.span(), "a handler arm is an arrow function"));
            };
            let name = self.property_name(property)?;
            let Expression::ArrowFunctionExpression(arrow) = &property.value else {
                return Err(self.error(property.span, "a handler arm is an arrow function"));
            };
            match name.as_str() {
                "return" => lowered.clauses.return_clause = Some(Box::new(self.arrow(arrow)?)),
                "finally" => lowered.clauses.finally = Some(Box::new(self.arm(arrow)?.1)),
                _ => {
                    let (params, body) = self.arm(arrow)?;
                    lowered.arms.push(HandlerArm {
                        operation: name,
                        params,
                        body,
                        position: Some(self.at(property.key.span())),
                    });
                }
            }
        }
        Ok(lowered)
    }

    /// An arm's parameter names and body, lowered as a function whose
    /// assignments to enclosing `let`s write handler state.
    fn arm(&mut self, arrow: &ArrowFunctionExpression<'_>) -> Result<(Vec<String>, Expr)> {
        if arrow
            .params
            .items
            .iter()
            .any(|parameter| parameter.type_annotation.is_some())
            || arrow.return_type.is_some()
        {
            return Err(self.error(
                arrow.params.span,
                "a handler arm's parameters take their types from the operation",
            ));
        }
        self.arm_scoped(|lowerer| {
            let params = lowerer
                .parameters(&arrow.params)?
                .into_iter()
                .map(|parameter| parameter.name)
                .collect();
            Ok((params, lowerer.arrow_body(arrow, None)?))
        })
    }
}

const PERFORM: &str = "`perform` names its effect: `perform<E>().op(…)`";
const HANDLER: &str = "`handler<E>(arms)` names its effect and takes one object of arms";
const HANDLE: &str = "`handle<E>(arms, () => body)` names its effect and takes its arms and a body";
