//! TypeScript expressions lower directly to canonical expression nodes.
//! Implements [FLAVOR-TS-EXPRESSIONS], [FLAVOR-TS-FUNCTIONS].
use super::context::{apply, Expected, Lowerer, Result};
use super::statements::Body;
use osprey_ast::{Expr, InterpolatedPart, Parameter, SymbolPath};
use oxc_ast::ast::{
    Argument, ArrayExpression, ArrowFunctionBody, ArrowFunctionExpression, BinaryExpression,
    CallExpression, Expression, FormalParameters, IdentifierReference, LogicalExpression,
    NumericLiteral, StaticMemberExpression, StringLiteral, TemplateLiteral, UnaryExpression,
};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator};

impl Lowerer<'_> {
    pub(super) fn expr(&mut self, node: &Expression<'_>) -> Result<Expr> {
        self.expr_as(node, None)
    }

    /// Lower `node`, constructing an object literal as `expected` when the
    /// syntax supplies a record type for it.
    pub(super) fn expr_as(
        &mut self,
        node: &Expression<'_>,
        expected: Option<&Expected>,
    ) -> Result<Expr> {
        match node {
            Expression::ParenthesizedExpression(inner) => self.expr_as(&inner.expression, expected),
            Expression::ObjectExpression(object) => self.object(object, expected),
            Expression::TSSatisfiesExpression(satisfies) => {
                let named = self.expected_type(&satisfies.type_annotation);
                self.expr_as(&satisfies.expression, named.as_ref().or(expected))
            }
            Expression::ConditionalExpression(conditional) => Ok(crate::desugar::bool_match(
                self.expr(&conditional.test)?,
                self.expr_as(&conditional.consequent, expected)?,
                self.expr_as(&conditional.alternate, expected)?,
            )),
            other => self.plain(other),
        }
    }

    fn plain(&mut self, node: &Expression<'_>) -> Result<Expr> {
        match node {
            Expression::BooleanLiteral(literal) => Ok(Expr::Bool(literal.value)),
            Expression::NumericLiteral(literal) => self.number(literal),
            Expression::StringLiteral(literal) => self.string(literal).map(Expr::Str),
            Expression::TemplateLiteral(template) => self.template(template),
            Expression::Identifier(identifier) => self.identifier(identifier),
            Expression::ArrayExpression(array) => self.array(array),
            Expression::ArrowFunctionExpression(arrow) => self.arrow(arrow),
            Expression::CallExpression(call) => self.call(call),
            Expression::StaticMemberExpression(member) => self.member(member),
            Expression::ComputedMemberExpression(member) => Ok(Expr::Index {
                target: Box::new(self.expr(&member.object)?),
                index: Box::new(self.expr(&member.expression)?),
            }),
            Expression::BinaryExpression(binary) => self.binary(binary),
            Expression::LogicalExpression(logical) => self.logical(logical),
            Expression::UnaryExpression(unary) => self.unary(unary),
            other => self.unsupported(other.span(), rejected(other)),
        }
    }

    fn number(&self, literal: &NumericLiteral<'_>) -> Result<Expr> {
        let raw = self.text(literal.span).replace('_', "");
        if raw.contains(['.', 'e', 'E']) && !raw.starts_with("0x") && !raw.starts_with("0X") {
            return Ok(Expr::Float(literal.value));
        }
        let parsed = match raw.get(..2) {
            Some("0x" | "0X") => i64::from_str_radix(raw.get(2..).unwrap_or_default(), 16),
            Some("0o" | "0O") => i64::from_str_radix(raw.get(2..).unwrap_or_default(), 8),
            Some("0b" | "0B") => i64::from_str_radix(raw.get(2..).unwrap_or_default(), 2),
            _ => raw.parse::<i64>(),
        };
        parsed.map(Expr::Integer).map_err(|error| {
            self.error(
                literal.span,
                format!("integer literal is outside the signed 64-bit range: {error}"),
            )
        })
    }

    pub(super) fn string(&self, literal: &StringLiteral<'_>) -> Result<String> {
        literal
            .value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| self.error(literal.span, "a string literal must be valid Unicode"))
    }

    fn template(&mut self, template: &TemplateLiteral<'_>) -> Result<Expr> {
        let mut parts = Vec::new();
        let mut expressions = template.expressions.iter();
        for quasi in &template.quasis {
            let text = cooked(quasi)
                .ok_or_else(|| self.error(quasi.span, "invalid template literal escape"))?;
            if !text.is_empty() {
                parts.push(InterpolatedPart::Text(text.to_owned()));
            }
            if let Some(expression) = expressions.next() {
                parts.push(InterpolatedPart::Expr(self.expr(expression)?));
            }
        }
        Ok(match parts.as_slice() {
            [] => Expr::Str(String::new()),
            [InterpolatedPart::Text(text)] => Expr::Str(text.clone()),
            _ => Expr::InterpolatedStr(parts),
        })
    }

    fn identifier(&mut self, identifier: &IdentifierReference<'_>) -> Result<Expr> {
        let name = identifier.name.as_str();
        self.note_use(name);
        if name == "undefined" && !self.bound_in_function(name) {
            return self.unsupported(identifier.span, "`undefined`");
        }
        Ok(Expr::Identifier(name.to_owned()))
    }

    fn array(&mut self, array: &ArrayExpression<'_>) -> Result<Expr> {
        let elements = array
            .elements
            .iter()
            .map(|element| match element.as_expression() {
                Some(expression) => self.expr(expression),
                None => self.unsupported(element.span(), "spread or hole in an array literal"),
            })
            .collect::<Result<_>>()?;
        Ok(Expr::List(elements, Some(self.at(array.span))))
    }

    fn arrow(&mut self, arrow: &ArrowFunctionExpression<'_>) -> Result<Expr> {
        if arrow.r#async {
            return self.unsupported(arrow.span, "`async` function; use fibers");
        }
        if let Some(parameters) = &arrow.type_parameters {
            return self.unsupported(parameters.span, "generic arrow function");
        }
        let position = Some(self.at(arrow.span));
        self.scoped(true, true, |lowerer| {
            let parameters = lowerer.parameters(&arrow.params)?;
            let return_type = lowerer.annotation(arrow.return_type.as_deref())?;
            let returns = lowerer.expected(arrow.return_type.as_deref());
            let body = match &arrow.body {
                ArrowFunctionBody::FunctionBody(body) => {
                    Body::Statements(&body.statements, body.span)
                }
                other => match other.as_expression() {
                    Some(expression) => Body::Expression(expression),
                    None => return lowerer.unsupported(arrow.span, "arrow function body"),
                },
            };
            let body = lowerer.function_body(&arrow.params, returns, &body)?;
            Ok(Expr::Lambda {
                parameters,
                return_type,
                body: Box::new(body),
                position,
            })
        })
    }

    pub(super) fn parameters(
        &mut self,
        parameters: &FormalParameters<'_>,
    ) -> Result<Vec<Parameter>> {
        if let Some(rest) = &parameters.rest {
            return self.unsupported(rest.span, "rest parameter");
        }
        parameters
            .items
            .iter()
            .map(|parameter| {
                if parameter.initializer.is_some() || parameter.optional {
                    return self.unsupported(parameter.span, "default or optional parameter");
                }
                let name = self.binding_name(&parameter.pattern)?;
                self.bind(&name);
                Ok(Parameter {
                    name,
                    ty: self.annotation(parameter.type_annotation.as_deref())?,
                    inline_constraint: false,
                })
            })
            .collect()
    }

    pub(super) fn arguments(&mut self, arguments: &[Argument<'_>]) -> Result<Vec<Expr>> {
        arguments
            .iter()
            .map(|argument| match argument.as_expression() {
                Some(expression) => self.expr(expression),
                None => self.unsupported(argument.span(), "spread argument"),
            })
            .collect()
    }

    fn call(&mut self, call: &CallExpression<'_>) -> Result<Expr> {
        if call.optional {
            return self.unsupported(call.span, "`?.` optional call");
        }
        if let Some(value) = self.intrinsic(call)? {
            return Ok(value);
        }
        let arguments = self.arguments(&call.arguments)?;
        let type_args = self.type_arguments(call.type_arguments.as_deref())?;
        if let Expression::StaticMemberExpression(member) = &call.callee {
            if type_args.is_empty() {
                return self.method(member, arguments);
            }
        }
        let function = self.callee(&call.callee)?;
        Ok(Expr::Call {
            function: Box::new(applied(function, type_args, self.at(call.callee.span()))),
            arguments,
            named_arguments: Vec::new(),
        })
    }

    /// A called name: a package default import names its entry function.
    fn callee(&mut self, callee: &Expression<'_>) -> Result<Expr> {
        if let Expression::Identifier(identifier) = callee {
            let name = identifier.name.as_str();
            if let Some(Some(entry)) = self.module(name) {
                return Ok(Expr::Path(SymbolPath::new([name, entry.as_str()])));
            }
        }
        self.expr(callee)
    }

    fn method(
        &mut self,
        member: &StaticMemberExpression<'_>,
        arguments: Vec<Expr>,
    ) -> Result<Expr> {
        Ok(match self.member(member)? {
            Expr::FieldAccess { target, field } => Expr::MethodCall {
                target,
                method: field,
                arguments,
                named_arguments: Vec::new(),
            },
            function => Expr::Call {
                function: Box::new(function),
                arguments,
                named_arguments: Vec::new(),
            },
        })
    }

    fn member(&mut self, member: &StaticMemberExpression<'_>) -> Result<Expr> {
        if member.optional {
            return self.unsupported(member.span, "`?.` optional chaining");
        }
        let field = member.property.name.as_str();
        if let Some(binder) = self.narrowed_field(&member.object, field) {
            return Ok(Expr::Identifier(binder));
        }
        Ok(match self.module_or_value(&member.object)? {
            Expr::Path(mut path) => {
                path.segments.push(field.to_owned());
                Expr::Path(path)
            }
            target if field == "length" => apply("length", vec![target]),
            target => Expr::FieldAccess {
                target: Box::new(target),
                field: field.to_owned(),
            },
        })
    }

    /// An import-bound module alias is a path head; anything else is a value.
    fn module_or_value(&mut self, object: &Expression<'_>) -> Result<Expr> {
        if let Expression::Identifier(identifier) = object {
            let name = identifier.name.as_str();
            if self.module(name).is_some() {
                return Ok(Expr::Path(SymbolPath::single(name)));
            }
        }
        self.expr(object)
    }

    fn module(&self, name: &str) -> Option<&Option<String>> {
        self.modules
            .get(name)
            .filter(|_| !self.bound_in_function(name) || self.file_scope())
    }

    fn narrowed_field(&mut self, object: &Expression<'_>, field: &str) -> Option<String> {
        let Expression::Identifier(identifier) = object else {
            return None;
        };
        let narrowing = self
            .narrowed
            .iter_mut()
            .rev()
            .find(|narrowing| narrowing.subject == identifier.name.as_str())?;
        if !narrowing.fields.iter().any(|known| known == field) {
            narrowing.fields.push(field.to_owned());
        }
        Some(field.to_owned())
    }

    fn binary(&mut self, binary: &BinaryExpression<'_>) -> Result<Expr> {
        let op = match binary.operator {
            BinaryOperator::StrictEquality => "==",
            BinaryOperator::StrictInequality => "!=",
            BinaryOperator::LessThan => "<",
            BinaryOperator::LessEqualThan => "<=",
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::GreaterEqualThan => ">=",
            BinaryOperator::Addition => "+",
            BinaryOperator::Subtraction => "-",
            BinaryOperator::Multiplication => "*",
            BinaryOperator::Division => "/",
            BinaryOperator::Remainder => "%",
            BinaryOperator::Equality | BinaryOperator::Inequality => {
                return self.unsupported(binary.span, "`==`/`!=`; write `===`/`!==`");
            }
            _ => return self.unsupported(binary.span, "operator"),
        };
        self.operation(op, &binary.left, &binary.right, binary.span)
    }

    fn logical(&mut self, logical: &LogicalExpression<'_>) -> Result<Expr> {
        let op = match logical.operator {
            LogicalOperator::And => "&&",
            LogicalOperator::Or => "||",
            LogicalOperator::Coalesce => return self.unsupported(logical.span, "`??`"),
        };
        self.operation(op, &logical.left, &logical.right, logical.span)
    }

    fn operation(
        &mut self,
        op: &str,
        left: &Expression<'_>,
        right: &Expression<'_>,
        span: Span,
    ) -> Result<Expr> {
        Ok(Expr::Binary {
            op: op.to_owned(),
            left: Box::new(self.expr(left)?),
            right: Box::new(self.expr(right)?),
            position: Some(self.at(span)),
        })
    }

    fn unary(&mut self, unary: &UnaryExpression<'_>) -> Result<Expr> {
        let operand = self.expr(&unary.argument)?;
        match unary.operator {
            UnaryOperator::UnaryNegation => Ok(Expr::negated(operand)),
            UnaryOperator::LogicalNot => Ok(Expr::Unary {
                op: "!".to_owned(),
                operand: Box::new(operand),
            }),
            _ => self.unsupported(unary.span, "unary operator"),
        }
    }
}

fn applied(
    function: Expr,
    type_args: Vec<osprey_ast::TypeExpr>,
    position: osprey_ast::Position,
) -> Expr {
    if type_args.is_empty() {
        return function;
    }
    Expr::TypeApply {
        function: Box::new(function),
        type_args,
        position: Some(position),
    }
}

/// A template segment's decoded text; `None` for an invalid escape.
fn cooked<'a>(quasi: &oxc_ast::ast::TemplateElement<'a>) -> Option<&'a str> {
    quasi.value.cooked?.as_str()
}

/// The name a rejected expression form is reported under.
fn rejected(node: &Expression<'_>) -> &'static str {
    match node {
        Expression::NullLiteral(_) => "`null`",
        Expression::AwaitExpression(_) => "`await`; use fibers",
        Expression::NewExpression(_) => "`new`",
        Expression::ThisExpression(_) => "`this`",
        Expression::AssignmentExpression(_) => "assignment expression",
        Expression::UpdateExpression(_) => "`++`/`--`",
        Expression::SequenceExpression(_) => "comma operator",
        Expression::TSAsExpression(_) | Expression::TSTypeAssertion(_) => "type assertion",
        Expression::TSNonNullExpression(_) => "non-null assertion",
        Expression::FunctionExpression(_) => "`function` expression; write an arrow function",
        Expression::ClassExpression(_) => "`class`",
        Expression::RegExpLiteral(_) => "regular-expression literal",
        Expression::TaggedTemplateExpression(_) => "tagged template",
        Expression::YieldExpression(_) => "`yield`",
        Expression::ChainExpression(_) => "`?.` optional chaining",
        Expression::BigIntLiteral(_) => "`bigint` literal",
        _ => "expression form",
    }
}
