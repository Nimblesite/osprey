//! Pattern rewriting: binders stay local, constructors are linked to their
//! declarations, and a payload-free variant written as a bare name is the
//! constructor it names in both flavors. Implements [MODULES-RESOLUTION].

use crate::model::DeclKind;
use crate::resolve::{Context, Locals, Resolver};
use osprey_ast::{MatchArm, Pattern};

impl Resolver<'_> {
    pub(crate) fn rewrite_pattern_arms(
        &mut self,
        arms: &mut [MatchArm],
        context: &Context,
        locals: &mut Locals,
    ) {
        for arm in arms {
            let saved = locals.clone();
            self.rewrite_pattern(&mut arm.pattern, context, locals);
            self.rewrite_expr(&mut arm.body, context, locals);
            *locals = saved;
        }
    }

    fn rewrite_pattern(&mut self, pattern: &mut Pattern, context: &Context, locals: &mut Locals) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Literal(value) => self.rewrite_expr(value, context, locals),
            Pattern::Constructor {
                name,
                fields,
                sub_patterns,
            } => {
                self.rewrite_constructor_name(name, context, "destructured");
                locals.values.extend(fields.iter().cloned());
                for nested in sub_patterns {
                    self.rewrite_pattern(nested, context, locals);
                }
            }
            Pattern::TypeAnnotated { name, ty } => {
                self.rewrite_type(ty, context, locals);
                let _ = locals.values.insert(name.clone());
            }
            Pattern::Structural { fields, .. } => locals
                .values
                .extend(fields.iter().map(|(_, binder)| binder.clone())),
            Pattern::List { elements, rest } => {
                for nested in elements {
                    self.rewrite_pattern(nested, context, locals);
                }
                locals.values.extend(rest.iter().cloned());
            }
            Pattern::Binding(name) => match self.nullary_constructor(name, context) {
                // The Default parser spells a payload-free variant pattern
                // (`Dry => …`) as a binding; the checker reads it as the
                // constructor, so the resolver must link it as one or the
                // renamed type keeps a variant no pattern can name.
                Some(constructor) => *pattern = constructor,
                None => locals.values.extend(std::iter::once(name.clone())),
            },
        }
    }

    /// The constructor pattern a bare binding stands for when a payload-free
    /// variant of that name is declared, linked to its renamed declaration.
    fn nullary_constructor(&mut self, name: &str, context: &Context) -> Option<Pattern> {
        let key = self.resolve_value_key(name, context)?;
        let declared = self.graph.declarations.get(&key)?.kind == DeclKind::Constructor;
        declared.then(|| Pattern::Constructor {
            name: self.link_name(&key, false),
            fields: Vec::new(),
            sub_patterns: Vec::new(),
        })
    }
}
