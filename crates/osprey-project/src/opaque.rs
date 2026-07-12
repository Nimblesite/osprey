//! Explicit flat-backend boundary for opaque manifest aliases.

use crate::resolve::{Context, Resolver};
use osprey_ast::Stmt;

impl Resolver<'_> {
    pub(crate) fn reject_opaque_alias(&mut self, statement: &Stmt, context: &Context) {
        let Stmt::Type { name, position, .. } = statement else {
            return;
        };
        self.error(
            context.source,
            *position,
            format!(
                "opaque alias `{name}` is unsupported by the flat checker; representation would leak"
            ),
        );
        let mut rewritten = statement.clone();
        self.rewrite_declaration(&mut rewritten, context, true, false);
        self.program.push(rewritten);
    }
}
