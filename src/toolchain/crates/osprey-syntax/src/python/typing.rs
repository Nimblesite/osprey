//! PEP annotation vocabulary is frontend metadata, never an executed import.
//! Implements [FLAVOR-PYTHON-TYPING].
use super::context::{children, Lowerer, Result};
use osprey_ast::TypeExpr;
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn binding_type(&self, node: Option<Node<'_>>) -> Result<(Option<TypeExpr>, bool)> {
        let Some(node) = node else {
            return Ok((None, false));
        };
        let inner = Self::annotation_part(node, "type")?;
        let name = Self::annotation_part(inner, "generic_type")?;
        if self.typing_name(self.text(name)) == Some("Final") {
            return self.final_type(inner);
        }
        if self.text(name) == "mut" {
            return Err(Self::error(
                name,
                "use standard typed assignment or typing.Final, not a mut annotation",
            ));
        }
        Ok((Some(self.type_expr(node)?), false))
    }

    fn final_type(&self, node: Node<'_>) -> Result<(Option<TypeExpr>, bool)> {
        if node.kind() == "identifier" {
            return Ok((None, true));
        }
        let args = Self::generic_arguments(node)?;
        let parts = children(args);
        let [part] = parts.as_slice() else {
            return Err(Self::error(
                args,
                "Final requires exactly one type argument",
            ));
        };
        Ok((Some(self.type_expr(*part)?), true))
    }

    fn annotation_part<'t>(node: Node<'t>, kind: &str) -> Result<Node<'t>> {
        if node.kind() == kind {
            Self::first(node)
        } else {
            Ok(node)
        }
    }

    pub(super) fn typing_import(&mut self, node: Node<'_>) -> Result<()> {
        let module = Self::field(node, "module_name")?;
        if !matches!(self.text(module), "typing" | "collections.abc") {
            return self.unsupported(node);
        }
        for item in children(node).into_iter().filter(|item| *item != module) {
            self.typing_item(module, item)?;
        }
        Ok(())
    }

    fn typing_item(&mut self, module: Node<'_>, item: Node<'_>) -> Result<()> {
        let name = item.child_by_field_name("name").unwrap_or(item);
        let original = self.text(name);
        if self.text(module) == "collections.abc" && original != "Callable" {
            return self.unsupported(item);
        }
        let canonical = annotation_name(original).ok_or_else(|| {
            Self::error(
                name,
                format!("typing form `{original}` has no Osprey representation"),
            )
        })?;
        let alias = item.child_by_field_name("alias").unwrap_or(name);
        self.bind(alias, false)?;
        self.import_typing_name(self.text(alias).into(), canonical.into());
        Ok(())
    }

    pub(super) fn callable_type(&self, node: Node<'_>) -> Result<TypeExpr> {
        let parts = children(node);
        let [parameters, result] = parts.as_slice() else {
            return self.unsupported(node);
        };
        let list = Self::annotation_part(*parameters, "type")?;
        if list.kind() != "list" {
            return Err(Self::error(
                list,
                "Callable requires an explicit parameter list",
            ));
        }
        let mut ty = TypeExpr::named("fn");
        ty.is_function = true;
        ty.parameter_types = self.type_arguments(list, 0)?;
        ty.return_type = Some(Box::new(self.type_expr(*result)?));
        Ok(ty)
    }
}

fn annotation_name(name: &str) -> Option<&'static str> {
    match name {
        "Callable" => Some("fn"),
        "List" => Some("List"),
        "Dict" => Some("Map"),
        "Any" => Some("any"),
        "Final" => Some("Final"),
        _ => None,
    }
}
