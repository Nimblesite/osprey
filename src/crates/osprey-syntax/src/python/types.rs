//! Python annotation spelling with the shared canonical type representation.
use super::context::{children, position, Lowerer, Result};
use crate::{AnnotationEdit, AnnotationTarget, BindingKind, SourceEdit};
use osprey_ast::{Parameter, TypeExpr};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn type_expr(&self, node: Node<'_>) -> Result<TypeExpr> {
        let mut ty = match node.kind() {
            "type" => return self.type_expr(Self::first(node)?),
            "identifier" | "member_type" | "attribute" => TypeExpr::named(self.type_name(node)),
            "none" => TypeExpr::named("Unit"),
            "generic_type" => self.generic_type(node)?,
            "subscript" => self.subscript_type(node)?,
            _ => return self.unsupported(node),
        };
        if ty.name == "Final" {
            return Err(Self::error(
                node,
                "Final is only valid as the outermost binding annotation",
            ));
        }
        ty.position = Some(position(node));
        Ok(ty)
    }

    fn subscript_type(&self, node: Node<'_>) -> Result<TypeExpr> {
        let mut ty = TypeExpr::named(self.type_name(Self::field(node, "value")?));
        ty.generic_params = self.type_arguments(node, 1)?;
        Ok(ty)
    }

    fn generic_type(&self, node: Node<'_>) -> Result<TypeExpr> {
        let name = Self::first(node)?;
        let args = Self::generic_arguments(node)?;
        let head = self.type_name(name);
        if head == "fn" {
            return self.callable_type(args);
        }
        let mut ty = TypeExpr::named(head);
        ty.generic_params = self.type_arguments(args, 0)?;
        Ok(ty)
    }

    pub(super) fn generic_arguments(node: Node<'_>) -> Result<Node<'_>> {
        children(node)
            .into_iter()
            .find(|n| n.kind() == "type_parameter")
            .ok_or_else(|| Self::error(node, "missing generic type arguments"))
    }

    fn type_name(&self, node: Node<'_>) -> String {
        self.typing_name(self.text(node))
            .unwrap_or_else(|| type_name(self.text(node)))
            .replace('.', "::")
    }

    pub(super) fn type_arguments(&self, node: Node<'_>, skip: usize) -> Result<Vec<TypeExpr>> {
        children(node)
            .into_iter()
            .skip(skip)
            .map(|arg| self.type_expr(arg))
            .collect()
    }

    pub(super) fn parameter(&mut self, node: Node<'_>, owner: Node<'_>) -> Result<Parameter> {
        let (name, ty) = match node.kind() {
            "identifier" => (node, None),
            "typed_parameter" => (
                Self::first(node)?,
                Some(self.type_expr(Self::field(node, "type")?)?),
            ),
            _ => return self.unsupported(node),
        };
        self.bind(name, false)?;
        self.binding(name, owner, BindingKind::Parameter);
        self.parameter_annotation(node, name, owner);
        Ok(Parameter {
            name: self.text(name).into(),
            ty,
            inline_constraint: false,
        })
    }

    fn parameter_annotation(&mut self, node: Node<'_>, name: Node<'_>, owner: Node<'_>) {
        if let Some(annotation) = node.child_by_field_name("type") {
            self.annotation(
                annotation,
                owner,
                AnnotationTarget::Parameter {
                    name: self.text(name).into(),
                    index: Self::parameter_index(node),
                },
            );
        }
    }

    fn parameter_index(node: Node<'_>) -> usize {
        node.parent().map_or(0, |parent| {
            children(parent)
                .iter()
                .position(|n| *n == node)
                .unwrap_or(0)
        })
    }

    pub(super) fn annotation(&mut self, node: Node<'_>, owner: Node<'_>, target: AnnotationTarget) {
        let before = node
            .prev_sibling()
            .filter(|sibling| matches!(sibling.kind(), ":" | "->"));
        let start = before.map_or(node.start_byte(), |token| token.start_byte());
        self.annotations.push(AnnotationEdit {
            position: position(node),
            owner_position: position(owner),
            target,
            highlight: node.byte_range(),
            edits: vec![SourceEdit {
                range: start..node.end_byte(),
                new_text: String::new(),
            }],
        });
    }
}

fn type_name(name: &str) -> &str {
    match name {
        "str" => "string",
        "list" => "List",
        "dict" => "Map",
        other => other,
    }
}
