//! Object literals as record values, and the calls the ambient library
//! defines as intrinsics. Implements [FLAVOR-TS-UNIONS], [FLAVOR-TS-LIB].
use super::context::{apply, Expected, Lowerer, Result};
use super::types::{variant_name, KIND};
use osprey_ast::{Expr, FieldAssignment};
use oxc_ast::ast::{
    CallExpression, Expression, NewExpression, ObjectExpression, ObjectProperty,
    ObjectPropertyKind, PropertyKind,
};
use oxc_span::GetSpan;

/// The fields a `Result` constructor intrinsic fills. [FLAVOR-TS-UNIONS]
const RESULT_CONSTRUCTORS: [(&str, &str); 2] = [("Success", "value"), ("Error", "message")];

impl Lowerer<'_> {
    /// `{ a: 1 }` constructs the record its context names; `{ kind: "circle", … }`
    /// constructs that variant; `{ ...record, a: 1 }` is a non-destructive update.
    pub(super) fn object(
        &mut self,
        object: &ObjectExpression<'_>,
        expected: Option<&Expected>,
    ) -> Result<Expr> {
        let mut fields = Vec::new();
        let mut variant = None;
        let mut base = None;
        for (index, property) in object.properties.iter().enumerate() {
            match property {
                ObjectPropertyKind::SpreadProperty(spread) => {
                    base = Some(self.update_base(&spread.argument, index)?);
                }
                ObjectPropertyKind::ObjectProperty(property) => {
                    let name = self.property_name(property)?;
                    if name == KIND {
                        variant = Some(self.kind_literal(&property.value)?);
                    } else {
                        let value = self.expr(&property.value)?;
                        fields.push(FieldAssignment { name, value });
                    }
                }
            }
        }
        if let Some(record) = base {
            return Ok(Expr::Update { record, fields });
        }
        let (name, type_args) = match (variant, expected) {
            // A variant with no fields is its bare constructor, as in Default.
            (Some(variant), _) if fields.is_empty() => return Ok(Expr::Identifier(variant)),
            (Some(variant), _) => (variant, Vec::new()),
            (None, Some(expected)) => (expected.name.clone(), expected.type_args.clone()),
            (None, None) => {
                return Err(self.error(
                    object.span,
                    "an object literal needs its record type: annotate the binding or the \
                     function's return type, or write `{ … } satisfies Type`",
                ));
            }
        };
        Ok(Expr::TypeConstructor {
            name,
            type_args,
            fields,
        })
    }

    fn update_base(&self, argument: &Expression<'_>, index: usize) -> Result<String> {
        match argument {
            Expression::Identifier(identifier) if index == 0 => {
                Ok(identifier.name.as_str().to_owned())
            }
            other => Err(self.error(
                other.span(),
                "a spread must come first and name the record binding it updates",
            )),
        }
    }

    fn property_name(&self, property: &ObjectProperty<'_>) -> Result<String> {
        if property.method || property.computed || property.kind != PropertyKind::Init {
            return self.unsupported(property.span, "method, accessor or computed property");
        }
        self.key(&property.key)
    }

    fn kind_literal(&self, value: &Expression<'_>) -> Result<String> {
        match value {
            Expression::StringLiteral(kind) => Ok(variant_name(&self.string(kind)?)),
            other => Err(self.error(other.span(), "`kind` must be a string literal")),
        }
    }

    /// A call the ambient library defines syntactically, or `None` for an
    /// ordinary call.
    pub(super) fn intrinsic(&mut self, call: &CallExpression<'_>) -> Result<Option<Expr>> {
        match &call.callee {
            Expression::Identifier(callee) if !self.bound_in_function(callee.name.as_str()) => {
                let field = RESULT_CONSTRUCTORS
                    .iter()
                    .find(|(name, _)| *name == callee.name.as_str());
                match field {
                    Some((name, field)) => self.result_constructor(call, name, field).map(Some),
                    None => Ok(None),
                }
            }
            Expression::StaticMemberExpression(member) => match &member.object {
                Expression::Identifier(object)
                    if object.name.as_str() == "console"
                        && member.property.name.as_str() == "log"
                        && !self.bound_in_function("console") =>
                {
                    Ok(Some(apply("print", vec![self.only_argument(call)?])))
                }
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// `new Map<string, T>()` is the empty map literal. [FLAVOR-TS-LIB]
    pub(super) fn new_expression(&self, node: &NewExpression<'_>) -> Result<Expr> {
        match &node.callee {
            Expression::Identifier(callee)
                if callee.name.as_str() == "Map" && node.arguments.is_empty() =>
            {
                Ok(Expr::Map(Vec::new()))
            }
            _ => self.unsupported(node.span, "`new`; only `new Map()` constructs a value"),
        }
    }

    fn result_constructor(
        &mut self,
        call: &CallExpression<'_>,
        name: &str,
        field: &str,
    ) -> Result<Expr> {
        Ok(Expr::TypeConstructor {
            name: name.to_owned(),
            type_args: Vec::new(),
            fields: vec![FieldAssignment {
                name: field.to_owned(),
                value: self.only_argument(call)?,
            }],
        })
    }

    fn only_argument(&mut self, call: &CallExpression<'_>) -> Result<Expr> {
        let mut arguments = self.arguments(&call.arguments)?;
        match (arguments.pop(), arguments.is_empty()) {
            (Some(argument), true) => Ok(argument),
            _ => Err(self.error(call.span, "this call takes exactly one argument")),
        }
    }
}
