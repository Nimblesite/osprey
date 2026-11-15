//! TypeScript annotations and declarations as canonical Osprey types.
//! Implements [FLAVOR-TS-TYPES], [FLAVOR-TS-UNIONS].
use super::context::{Expected, Lowerer, Result};
use osprey_ast::{Stmt, TypeExpr, TypeField, TypeParam, TypeVariant, Variance};
use oxc_ast::ast::{
    FormalParameters, PropertyKey, TSInterfaceDeclaration, TSLiteral, TSSignature, TSType,
    TSTypeAliasDeclaration, TSTypeAnnotation, TSTypeName, TSTypeOperatorOperator,
    TSTypeParameterDeclaration, TSTypeParameterInstantiation,
};
use oxc_span::{GetSpan, Span};

/// The discriminant property of a TypeScript-flavor union. [FLAVOR-TS-UNIONS]
pub(super) const KIND: &str = "kind";

/// `"circle"` names variant `Circle`; `"not-found"` names `NotFound`.
pub(super) fn variant_name(kind: &str) -> String {
    kind.split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let first = chars.next().into_iter().flat_map(char::to_uppercase);
            first.chain(chars).collect::<String>()
        })
        .collect()
}

impl Lowerer<'_> {
    pub(super) fn annotation(
        &self,
        annotation: Option<&TSTypeAnnotation<'_>>,
    ) -> Result<Option<TypeExpr>> {
        annotation
            .map(|annotation| self.type_expr(&annotation.type_annotation))
            .transpose()
    }

    pub(super) fn type_expr(&self, ty: &TSType<'_>) -> Result<TypeExpr> {
        let mut lowered = match ty {
            TSType::TSNumberKeyword(_) => TypeExpr::named("float"),
            TSType::TSStringKeyword(_) => TypeExpr::named("string"),
            TSType::TSBooleanKeyword(_) => TypeExpr::named("bool"),
            TSType::TSVoidKeyword(_) => TypeExpr::named("Unit"),
            TSType::TSUnknownKeyword(_) => TypeExpr::named("any"),
            TSType::TSArrayType(array) => list_of(self.type_expr(&array.element_type)?),
            TSType::TSParenthesizedType(inner) => self.type_expr(&inner.type_annotation)?,
            TSType::TSTypeOperatorType(operator)
                if operator.operator == TSTypeOperatorOperator::Readonly =>
            {
                self.type_expr(&operator.type_annotation)?
            }
            TSType::TSTypeReference(reference) => {
                let mut named = TypeExpr::named(self.type_name(&reference.type_name)?);
                named.generic_params = self.type_arguments(reference.type_arguments.as_deref())?;
                named
            }
            TSType::TSFunctionType(function) => self.function_type(
                &function.params,
                function.return_type.type_annotation.span(),
                Some(&function.return_type),
            )?,
            other => return self.rejected_type(other),
        };
        lowered.position = Some(self.at(ty.span()));
        Ok(lowered)
    }

    fn rejected_type<T>(&self, ty: &TSType<'_>) -> Result<T> {
        let construct = match ty {
            TSType::TSAnyKeyword(_) => "`any`",
            TSType::TSNullKeyword(_) => "`null`",
            TSType::TSUndefinedKeyword(_) => "`undefined`",
            TSType::TSUnionType(_) => "union type outside a `kind`-discriminated declaration",
            TSType::TSIntersectionType(_) => "intersection type",
            TSType::TSTypeLiteral(_) => "anonymous object type; declare an interface",
            TSType::TSLiteralType(_) => "literal type outside a `kind` discriminant",
            _ => "type form",
        };
        self.unsupported(ty.span(), construct)
    }

    pub(super) fn type_arguments(
        &self,
        arguments: Option<&TSTypeParameterInstantiation<'_>>,
    ) -> Result<Vec<TypeExpr>> {
        arguments.map_or_else(
            || Ok(Vec::new()),
            |arguments| {
                arguments
                    .params
                    .iter()
                    .map(|argument| self.type_expr(argument))
                    .collect()
            },
        )
    }

    fn type_name(&self, name: &TSTypeName<'_>) -> Result<String> {
        match name {
            TSTypeName::IdentifierReference(identifier) => Ok(identifier.name.as_str().to_owned()),
            TSTypeName::QualifiedName(qualified) => Ok(format!(
                "{}::{}",
                self.type_name(&qualified.left)?,
                qualified.right.name.as_str()
            )),
            TSTypeName::ThisExpression(this) => self.unsupported(this.span, "`this` type"),
        }
    }

    fn function_type(
        &self,
        parameters: &FormalParameters<'_>,
        span: Span,
        result: Option<&TSTypeAnnotation<'_>>,
    ) -> Result<TypeExpr> {
        let mut ty = TypeExpr::named("fn");
        ty.is_function = true;
        ty.parameter_types = parameters
            .items
            .iter()
            .map(|parameter| {
                self.annotation(parameter.type_annotation.as_deref())?
                    .ok_or_else(|| self.error(parameter.span, "a signature parameter needs a type"))
            })
            .collect::<Result<_>>()?;
        let result = self
            .annotation(result)?
            .ok_or_else(|| self.error(span, "a signature needs a result type"))?;
        ty.return_type = Some(Box::new(result));
        Ok(ty)
    }

    /// The record an annotation names, for an object literal bound under it.
    pub(super) fn expected(&self, annotation: Option<&TSTypeAnnotation<'_>>) -> Option<Expected> {
        self.expected_type(&annotation?.type_annotation)
    }

    pub(super) fn expected_type(&self, ty: &TSType<'_>) -> Option<Expected> {
        let TSType::TSTypeReference(reference) = ty else {
            return None;
        };
        Some(Expected {
            name: self.type_name(&reference.type_name).ok()?,
            type_args: self
                .type_arguments(reference.type_arguments.as_deref())
                .ok()?,
        })
    }

    pub(super) fn type_parameters(
        &self,
        parameters: Option<&TSTypeParameterDeclaration<'_>>,
    ) -> Result<Vec<TypeParam>> {
        let Some(parameters) = parameters else {
            return Ok(Vec::new());
        };
        parameters
            .params
            .iter()
            .map(|parameter| {
                if parameter.constraint.is_some() || parameter.default.is_some() {
                    return self.unsupported(parameter.span, "constrained type parameter");
                }
                Ok(TypeParam {
                    name: parameter.name.name.as_str().to_owned(),
                    variance: Variance::Invariant,
                })
            })
            .collect()
    }

    /// `interface Point { readonly x: number }` is a closed record.
    pub(super) fn interface(&mut self, node: &TSInterfaceDeclaration<'_>) -> Result<Stmt> {
        if let Some(heritage) = node.extends.first() {
            return self.unsupported(heritage.span, "interface `extends`");
        }
        let name = node.id.name.as_str();
        let variant = TypeVariant {
            name: name.to_owned(),
            fields: self.fields(&node.body.body)?,
        };
        self.data_type(
            name,
            node.type_parameters.as_deref(),
            vec![variant],
            None,
            node.span,
        )
    }

    /// `type Shape = { kind: "circle" … } | …` is a sum type; any other alias
    /// is a manifest alias.
    pub(super) fn type_alias(&mut self, node: &TSTypeAliasDeclaration<'_>) -> Result<Stmt> {
        let name = node.id.name.as_str();
        let parameters = node.type_parameters.as_deref();
        match &node.type_annotation {
            TSType::TSUnionType(union) => {
                let variants = union
                    .types
                    .iter()
                    .map(|member| self.union_member(member))
                    .collect::<Result<_>>()?;
                self.data_type(name, parameters, variants, None, node.span)
            }
            TSType::TSTypeLiteral(literal) => {
                let variant = TypeVariant {
                    name: name.to_owned(),
                    fields: self.fields(&literal.members)?,
                };
                self.data_type(name, parameters, vec![variant], None, node.span)
            }
            other => {
                let alias = Some(self.type_expr(other)?);
                self.data_type(name, parameters, Vec::new(), alias, node.span)
            }
        }
    }

    fn union_member(&self, member: &TSType<'_>) -> Result<TypeVariant> {
        let TSType::TSTypeLiteral(literal) = member else {
            return self.unsupported(member.span(), "union member without a `kind` property");
        };
        let name = literal
            .members
            .iter()
            .find_map(|signature| self.kind_of(signature))
            .ok_or_else(|| {
                self.error(
                    literal.span,
                    "a union member needs a `kind` property with a string-literal type",
                )
            })?;
        let fields = literal
            .members
            .iter()
            .filter(|signature| self.kind_of(signature).is_none())
            .map(|signature| self.field(signature))
            .collect::<Result<_>>()?;
        Ok(TypeVariant { name, fields })
    }

    /// The variant a `kind: "literal"` property signature names.
    fn kind_of(&self, signature: &TSSignature<'_>) -> Option<String> {
        let TSSignature::TSPropertySignature(property) = signature else {
            return None;
        };
        if self.key(&property.key).ok()? != KIND {
            return None;
        }
        let TSType::TSLiteralType(literal) = &property.type_annotation.as_deref()?.type_annotation
        else {
            return None;
        };
        let TSLiteral::StringLiteral(text) = &literal.literal else {
            return None;
        };
        Some(variant_name(text.value.as_str()?))
    }

    fn fields(&self, signatures: &[TSSignature<'_>]) -> Result<Vec<TypeField>> {
        signatures
            .iter()
            .map(|signature| self.field(signature))
            .collect()
    }

    fn field(&self, signature: &TSSignature<'_>) -> Result<TypeField> {
        let (name, ty) = match signature {
            TSSignature::TSPropertySignature(property) => {
                if property.optional || property.computed {
                    return self.unsupported(property.span, "optional or computed property");
                }
                let ty = self
                    .annotation(property.type_annotation.as_deref())?
                    .ok_or_else(|| self.error(property.span, "a field needs a type"))?;
                (self.key(&property.key)?, ty)
            }
            TSSignature::TSMethodSignature(method) => {
                if method.optional || method.computed || method.type_parameters.is_some() {
                    return self.unsupported(method.span, "optional, computed or generic method");
                }
                let ty =
                    self.function_type(&method.params, method.span, method.return_type.as_deref())?;
                (self.key(&method.key)?, ty)
            }
            other => return self.unsupported(other.span(), "index, call or construct signature"),
        };
        Ok(TypeField {
            name,
            ty: crate::desugar::field_type(&ty),
            constraint: None,
            effects: None,
        })
    }

    pub(super) fn key(&self, key: &PropertyKey<'_>) -> Result<String> {
        match key {
            PropertyKey::StaticIdentifier(identifier) => Ok(identifier.name.as_str().to_owned()),
            other => self.unsupported(other.span(), "computed or non-identifier property name"),
        }
    }

    fn data_type(
        &mut self,
        name: &str,
        parameters: Option<&TSTypeParameterDeclaration<'_>>,
        variants: Vec<TypeVariant>,
        alias: Option<TypeExpr>,
        span: Span,
    ) -> Result<Stmt> {
        self.bind(name);
        Ok(Stmt::Type {
            name: name.to_owned(),
            type_params: self.type_parameters(parameters)?,
            variants,
            alias,
            validation_func: None,
            opaque: false,
            doc: None,
            position: Some(self.at(span)),
        })
    }
}

fn list_of(element: TypeExpr) -> TypeExpr {
    let mut list = TypeExpr::named("List");
    list.generic_params = vec![element];
    list
}
