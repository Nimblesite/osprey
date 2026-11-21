//! TypeScript expressions lower directly to canonical expression nodes.
//! Implements [FLAVOR-TS-EXPRESSIONS], [FLAVOR-TS-FUNCTIONS].
use super::context::{apply, Expected, Lowerer, Result};
use super::statements::Body;
use osprey_ast::{Expr, InterpolatedPart, Parameter};
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
            Expression::NewExpression(node) => self.new_expression(node),
            Expression::BinaryExpression(binary) => self.binary(binary),
            Expression::LogicalExpression(logical) => self.logical(logical),
            Expression::UnaryExpression(unary) => self.unary(unary),
            other => self.unsupported(other.span(), rejected(other)),
        }
    }

    fn number(&self, literal: &NumericLiteral<'_>) -> Result<Expr> {
        let raw = self.text(literal.span).replace('_', "");
        if raw.starts_with('0') && raw.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
            return Err(self.error(
                literal.span,
                "legacy octal or leading-zero number: TypeScript 7 rejects it; write `0o17` or `17`",
            ));
        }
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

    pub(super) fn arrow(&mut self, arrow: &ArrowFunctionExpression<'_>) -> Result<Expr> {
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
            let body = lowerer.arrow_body(arrow, returns)?;
            Ok(Expr::Lambda {
                parameters,
                return_type,
                body: Box::new(body),
                position,
            })
        })
    }

    pub(super) fn arrow_body(
        &mut self,
        arrow: &ArrowFunctionExpression<'_>,
        returns: Option<Expected>,
    ) -> Result<Expr> {
        let body = match &arrow.body {
            ArrowFunctionBody::FunctionBody(body) => Body::Statements(&body.statements, body.span),
            other => match other.as_expression() {
                Some(expression) => Body::Expression(expression),
                None => return self.unsupported(arrow.span, "arrow function body"),
            },
        };
        self.function_body(&arrow.params, returns, &body)
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
                if let Some(list) = self.array_method(member, call.span, &arguments)? {
                    return Ok(list);
                }
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

    /// `xs.map(f)`, `xs.filter(p)` and `xs.reduce(f, initial)` are the list
    /// operations. JavaScript takes `reduce`'s initial value last; Osprey's
    /// `foldList` takes it before the function. A `reduce` with no initial
    /// value has no Osprey meaning for an empty list, so it is rejected.
    fn array_method(
        &mut self,
        member: &StaticMemberExpression<'_>,
        span: Span,
        arguments: &[Expr],
    ) -> Result<Option<Expr>> {
        let name = member.property.name.as_str();
        let operation = match (name, arguments) {
            ("map", [_]) => "mapList",
            ("filter", [_]) => "filterList",
            ("reduce", [_, _]) => "foldList",
            ("reduce", [_]) => {
                return self.unsupported(span, "`reduce` without an initial value");
            }
            _ => return Ok(None),
        };
        let target = self.expr(&member.object)?;
        let ordered = match arguments {
            [function, initial] => vec![target, initial.clone(), function.clone()],
            _ => std::iter::once(target)
                .chain(arguments.iter().cloned())
                .collect(),
        };
        Ok(Some(apply(operation, ordered)))
    }

    /// A called name: a package default import names its entry function.
    fn callee(&mut self, callee: &Expression<'_>) -> Result<Expr> {
        if let Expression::Identifier(identifier) = callee {
            if let Some(entry) = self.package(identifier.name.as_str()) {
                return Ok(Expr::Identifier(entry.to_owned()));
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
        if let Expression::Identifier(object) = &member.object {
            if self.package(object.name.as_str()).is_some() {
                return Ok(Expr::Identifier(field.to_owned()));
            }
        }
        Ok(match self.expr(&member.object)? {
            // TypeScript counts a string's UTF-16 code units, and Osprey has no
            // such count yet, so `.length` is a list's element count and a
            // string's `.length` fails to type-check. [FLAVOR-TS-EXPRESSIONS]
            target if field == "length" => apply("listLength", vec![target]),
            target => Expr::FieldAccess {
                target: Box::new(target),
                field: field.to_owned(),
            },
        })
    }

    /// The entry function of the package `name` is the default import of,
    /// unless a local shadows the import. `express.Router` is the package's
    /// own `Router`, in scope through the same import.
    fn package(&self, name: &str) -> Option<&str> {
        self.packages
            .get(name)
            .filter(|_| !self.bound_in_function(name))
            .map(String::as_str)
    }

    fn narrowed_field(&mut self, object: &Expression<'_>, field: &str) -> Option<String> {
        let Expression::Identifier(identifier) = object else {
            return None;
        };
        let at = self
            .narrowed
            .iter()
            .rposition(|narrowing| narrowing.subject == identifier.name.as_str())?;
        let narrowing = self.narrowed.get(at)?;
        if let Some((_, binder)) = narrowing.fields.iter().find(|(known, _)| known == field) {
            return Some(binder.clone());
        }
        let binder = self.field_binder(&narrowing.subject, field, narrowing.span);
        self.narrowed
            .get_mut(at)?
            .fields
            .push((field.to_owned(), binder.clone()));
        Some(binder)
    }

    fn binary(&mut self, binary: &BinaryExpression<'_>) -> Result<Expr> {
        let op = match binary.operator {
            BinaryOperator::Equality | BinaryOperator::Inequality => {
                return self.unsupported(binary.span, "`==`/`!=`; write `===`/`!==`");
            }
            operator => match binary_spelling(operator) {
                Some(op) => op,
                None => return self.unsupported(binary.span, "operator"),
            },
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

/// The Osprey spelling of a TypeScript binary operator Osprey has.
pub(super) fn binary_spelling(operator: BinaryOperator) -> Option<&'static str> {
    Some(match operator {
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
        _ => return None,
    })
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
