//! Closed record classes and checked foreign signatures; no Python objects.
//! Implements [FLAVOR-PYTHON-CLASS], [FLAVOR-PYTHON-FUNCTIONS].
use super::context::{children, position, Lowerer, Result};
use osprey_ast::{ExternParameter, Stmt, TypeExpr, TypeField, TypeParam, TypeVariant, Variance};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn type_alias(&mut self, node: Node<'_>) -> Result<Stmt> {
        let left = Self::field(node, "left")?;
        let name = self.identifier(Self::first(left)?)?;
        self.bind(name, false)?;
        self.data_type(
            node,
            name,
            Vec::new(),
            Some(self.type_expr(Self::field(node, "right")?)?),
        )
    }

    pub(super) fn decorated(&mut self, node: Node<'_>) -> Result<Stmt> {
        let definition = Self::field(node, "definition")?;
        let decorators = self.declaration_decorators(node, definition)?;
        let decorator = match decorators.as_slice() {
            [] => {
                return match definition.kind() {
                    "function_definition" => self.function(definition),
                    "class_definition" => self.record(definition),
                    _ => self.unsupported(definition),
                }
            }
            [decorator] => *decorator,
            _ => return self.unsupported(node),
        };
        self.decorated_statement(definition, decorator)
    }

    fn decorated_statement(&mut self, definition: Node<'_>, decorator: Node<'_>) -> Result<Stmt> {
        match (self.text(decorator).trim(), definition.kind()) {
            ("extern", "function_definition") => self.external(definition),
            ("effect", "class_definition") => self.effect(definition, osprey_ast::Stage::Dynamic),
            ("static_effect", "class_definition") => {
                self.effect(definition, osprey_ast::Stage::Static)
            }
            ("union", "class_definition") => self.union(definition),
            (_, "function_definition") => self.required_function(definition, decorator),
            (_, "class_definition") => self.handler(definition, decorator),
            _ => self.unsupported(definition),
        }
    }

    fn declaration_decorators<'t>(
        &mut self,
        node: Node<'t>,
        definition: Node<'t>,
    ) -> Result<Vec<Node<'t>>> {
        let mut decorators = Vec::new();
        for decorator in children(node)
            .into_iter()
            .filter(|n| n.kind() == "decorator")
        {
            let expression = Self::first(decorator)?;
            if self.text(expression) == "export" {
                self.mark_export(Self::field(definition, "name")?)?;
            } else {
                decorators.push(expression);
            }
        }
        Ok(decorators)
    }

    fn external(&mut self, node: Node<'_>) -> Result<Stmt> {
        self.signature_only(node)?;
        self.reject_field(node, "type_parameters")?;
        let name = self.declaration_name(node)?;
        self.scoped(true, |lowerer| {
            let parameters = lowerer.typed_parameters(node)?;
            let return_type = Some(lowerer.type_expr(Self::field(node, "return_type")?)?);
            Ok(Stmt::Extern {
                name: lowerer.text(name).into(),
                parameters,
                return_type,
                doc: lowerer.declaration_doc(node)?,
                position: Some(position(node)),
            })
        })
    }

    pub(super) fn typed_parameters(&mut self, node: Node<'_>) -> Result<Vec<ExternParameter>> {
        self.parameters(node)?
            .into_iter()
            .map(|param| {
                let ty = param.ty.ok_or_else(|| {
                    Self::error(
                        node,
                        format!("signature parameter `{}` requires a type", param.name),
                    )
                })?;
                Ok(ExternParameter {
                    name: param.name,
                    ty,
                })
            })
            .collect()
    }

    /// `@union class Shape:` declares an Osprey sum type: each immediately
    /// nested, undecorated record class supplies one variant's fields
    /// ([FLAVOR-PYTHON-CLASS]).
    fn union(&mut self, node: Node<'_>) -> Result<Stmt> {
        self.reject_field(node, "superclasses")?;
        let name = self.declaration_name(node)?;
        let variants = self
            .body_nodes(Self::field(node, "body")?)
            .into_iter()
            .map(|variant| self.variant(variant))
            .collect::<Result<_>>()?;
        self.data_type(node, name, variants, None)
    }

    fn variant(&mut self, node: Node<'_>) -> Result<TypeVariant> {
        self.reject_field(node, "superclasses")?;
        self.reject_field(node, "type_parameters")?;
        let name = self.field_name(Self::field(node, "name")?)?;
        let fields = self
            .body_nodes(Self::field(node, "body")?)
            .into_iter()
            .filter(|field| field.kind() != "pass_statement")
            .map(|field| self.record_field(field))
            .collect::<Result<_>>()?;
        Ok(TypeVariant {
            name: self.text(name).into(),
            fields,
        })
    }

    pub(super) fn record(&mut self, node: Node<'_>) -> Result<Stmt> {
        self.reject_field(node, "superclasses")?;
        let name = self.declaration_name(node)?;
        let fields = self
            .body_nodes(Self::field(node, "body")?)
            .into_iter()
            .map(|field| self.record_field(field))
            .collect::<Result<_>>()?;
        self.record_constructor(name);
        let variant = TypeVariant {
            name: self.text(name).into(),
            fields,
        };
        self.data_type(node, name, vec![variant], None)
    }

    fn data_type(
        &mut self,
        node: Node<'_>,
        name: Node<'_>,
        variants: Vec<TypeVariant>,
        alias: Option<TypeExpr>,
    ) -> Result<Stmt> {
        let doc = self.declaration_doc(node)?;
        Ok(Stmt::Type {
            name: self.text(name).into(),
            type_params: self.type_parameters(node)?,
            variants,
            alias,
            validation_func: None,
            opaque: false,
            doc,
            position: Some(position(node)),
        })
    }

    fn record_field(&mut self, node: Node<'_>) -> Result<TypeField> {
        if node.kind() == "function_definition" {
            return self.function_field(node);
        }
        let assignment = Self::first(node)?;
        if node.kind() != "expression_statement"
            || assignment.kind() != "assignment"
            || assignment.child_by_field_name("right").is_some()
        {
            return self.unsupported(node);
        }
        let name = self.field_name(Self::field(assignment, "left")?)?;
        let ty = self.type_expr(Self::field(assignment, "type")?)?;
        Ok(TypeField {
            name: self.text(name).into(),
            ty: render(&ty),
            constraint: None,
            effects: None,
        })
    }

    fn function_field(&mut self, node: Node<'_>) -> Result<TypeField> {
        self.signature_only(node)?;
        let name = self.field_name(Self::field(node, "name")?)?;
        self.scoped(true, |lowerer| {
            Ok(TypeField {
                name: lowerer.text(name).into(),
                ty: lowerer.signature_text(node)?,
                constraint: None,
                effects: None,
            })
        })
    }

    fn field_name<'t>(&self, node: Node<'t>) -> Result<Node<'t>> {
        let name = self.identifier(node)?;
        if self.text(name).starts_with("__") {
            return self.unsupported(name);
        }
        Ok(name)
    }

    pub(super) fn signature_only(&self, node: Node<'_>) -> Result<()> {
        Self::synchronous(node)?;
        let body = Self::field(node, "body")?;
        let items = self.body_nodes(body);
        if items.len() != 1
            || items
                .first()
                .is_none_or(|item| self.text(*item).trim() != "...")
        {
            return Err(Self::error(
                body,
                "a declaration signature must have exactly an `...` body",
            ));
        }
        Ok(())
    }

    pub(super) fn type_parameters(&self, node: Node<'_>) -> Result<Vec<TypeParam>> {
        let Some(params) = node.child_by_field_name("type_parameters") else {
            return Ok(Vec::new());
        };
        children(params)
            .into_iter()
            .map(|param| {
                let name = Self::first(param)?;
                if name.kind() != "identifier" {
                    return self.unsupported(param);
                }
                Ok(TypeParam {
                    name: self.text(name).into(),
                    variance: Variance::Invariant,
                })
            })
            .collect()
    }
}

pub(super) fn render(ty: &TypeExpr) -> String {
    if ty.is_function {
        let result = ty
            .return_type
            .as_deref()
            .map_or_else(|| "Unit".into(), render);
        return format!(
            "({}) -> {result}",
            ty.parameter_types
                .iter()
                .map(render)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    osprey_ast::effect_name::instantiated(&ty.name, &ty.generic_params)
}
