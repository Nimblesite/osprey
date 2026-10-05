//! The representation boundary of an opaque type ([MODULES-OPAQUE-TYPES]):
//! a constructor is usable only inside the owning module, and a manifest
//! alias reaches the checker as a nominal type whose representation only that
//! module may see.

use crate::model::SymbolKey;
use crate::resolve::{Context, Resolver};
use osprey_ast::Stmt;

impl Resolver<'_> {
    /// Hand an opaque manifest alias to the checker together with its
    /// representation. Every use keeps the alias's own name, so the checker
    /// decides, declaration by declaration, who may read through it.
    pub(crate) fn declare_opaque_alias(&mut self, statement: &Stmt, context: &Context) {
        let mut rewritten = statement.clone();
        self.rewrite_declaration(&mut rewritten, context, true, false);
        self.program.push(rewritten);
    }

    /// Link a constructor at a construction or pattern site, refusing an opaque
    /// type's representation outside its owning module. `action` names the use
    /// in the diagnostic: `constructed` or `destructured`.
    pub(crate) fn rewrite_constructor_name(
        &mut self,
        name: &mut String,
        context: &Context,
        action: &str,
    ) {
        let key = self.resolve_value_key(name, context);
        if let Some(key) = &key {
            if let Some(owner) = self.hidden_owner(key, context) {
                self.error(
                    context.source,
                    None,
                    format!(
                        "opaque type `{}` cannot be {action} outside module `{owner}`",
                        key.source_name()
                    ),
                );
            }
        }
        self.link_resolved(name, key.as_ref(), context, true);
    }

    /// The owning module's source name when `key` is opaque and `context`
    /// lies outside it; `None` when the representation is visible here.
    fn hidden_owner(&self, key: &SymbolKey, context: &Context) -> Option<String> {
        let info = self.graph.declarations.get(key)?;
        let inside = key.namespace == context.namespace && context.module.starts_with(&info.owner);
        (info.opaque && !inside)
            .then(|| SymbolKey::new(key.namespace.clone(), info.owner.clone()).source_name())
    }
}
