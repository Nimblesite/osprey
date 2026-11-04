//! Resolve canonical qualified paths, retaining field access after a value.
//! Implements [MODULES-RESOLUTION], [FLAVOR-PYTHON-INTEROP].
use crate::model::{DeclKind, SymbolKey};
use crate::resolve::{Context, Resolver};
use osprey_ast::Expr;

impl Resolver<'_> {
    pub(crate) fn qualified_expression(
        &mut self,
        segments: &[String],
        context: &Context,
    ) -> Option<Expr> {
        let (key, tail) = self.qualified_value_key(segments, context)?;
        let mut value = Expr::Identifier(key.source_name());
        self.replace_symbol_expression(&mut value, &key, context);
        Some(
            tail.into_iter()
                .fold(value, |target, field| Expr::FieldAccess {
                    target: Box::new(target),
                    field,
                }),
        )
    }

    fn module_key(&self, key: &SymbolKey) -> bool {
        self.graph
            .declarations
            .get(key)
            .is_some_and(|info| info.kind == DeclKind::Module)
    }

    fn qualified_value_key(
        &mut self,
        segments: &[String],
        context: &Context,
    ) -> Option<(SymbolKey, Vec<String>)> {
        match segments
            .first()
            .and_then(|name| self.resolve_bare(name, context, None))
        {
            Some(key) if !self.module_key(&key) => Some((key, segments.get(1..)?.to_vec())),
            _ => self.module_value_key(segments, context),
        }
    }

    fn module_value_key(
        &mut self,
        segments: &[String],
        context: &Context,
    ) -> Option<(SymbolKey, Vec<String>)> {
        for end in 2..=segments.len() {
            let key = self.resolve_path(segments.get(..end)?, context, None)?;
            if end == segments.len() || !self.module_key(&key) {
                return Some((key, segments.get(end..)?.to_vec()));
            }
        }
        None
    }
}
