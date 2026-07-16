//! Resolved value-symbol materialization and constant substitution.

use crate::model::{DeclKind, SymbolKey};
use crate::resolve::{Context, Resolver};
use osprey_ast::Expr;

impl Resolver<'_> {
    pub(crate) fn replace_symbol_expression(
        &mut self,
        expression: &mut Expr,
        key: &SymbolKey,
        context: &Context,
    ) {
        if let Some(declaration) = self.graph.declarations.get(key) {
            if declaration.state_owner {
                self.replace_state_cell(expression, key, context, &declaration.owner);
                return;
            }
        }
        if self.graph.constants.contains_key(key) {
            self.replace_constant(expression, key, context);
            return;
        }
        if self
            .graph
            .declarations
            .get(key)
            .is_some_and(|declaration| declaration.kind == DeclKind::Module)
        {
            self.error(
                context.source,
                None,
                format!("module `{}` is not a value", key.source_name()),
            );
            return;
        }
        *expression = Expr::Identifier(self.link_name(key, false));
    }

    fn replace_state_cell(
        &mut self,
        expression: &mut Expr,
        key: &SymbolKey,
        context: &Context,
        owner: &[String],
    ) {
        if key.namespace == context.namespace && context.module.starts_with(owner) {
            if let Some(name) = key.path.last() {
                *expression = Expr::Identifier(name.clone());
            }
        } else {
            self.error(
                context.source,
                None,
                format!(
                    "state cell `{}` cannot escape its owning module",
                    key.source_name()
                ),
            );
        }
    }

    fn replace_constant(&mut self, expression: &mut Expr, key: &SymbolKey, context: &Context) {
        if self.runtime_constants.contains(key) {
            self.error(
                context.source,
                None,
                format!(
                    "entry runtime binding `{}` cannot be captured by a project declaration",
                    key.source_name()
                ),
            );
        } else if let Some(value) = self.constant_value(key) {
            *expression = value;
        }
    }
}
