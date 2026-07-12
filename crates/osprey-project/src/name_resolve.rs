//! Logical path lookup with lexical precedence and boundary checks.
//! Implements [MODULES-RESOLUTION] and [MODULES-EXPORTS].

use crate::model::SymbolKey;
use crate::resolve::{Context, Resolver};
use osprey_ast::Position;

impl Resolver<'_> {
    pub fn resolve_bare(
        &mut self,
        name: &str,
        context: &Context,
        position: Option<Position>,
    ) -> Option<SymbolKey> {
        let scope = self.scopes.get(context.contribution);
        for depth in (0..=context.module.len()).rev() {
            let mut path = context.module.get(..depth).unwrap_or_default().to_vec();
            path.push(name.to_string());
            let key = SymbolKey::new(context.namespace.clone(), path);
            if self.symbol_visible(&key, context) {
                return Some(key);
            }
        }
        if scope.is_some_and(|scope| scope.is_ambiguous(name)) {
            self.error(
                context.source,
                position,
                format!("ambiguous imported name `{name}`"),
            );
            return None;
        }
        scope.and_then(|scope| scope.member(name)).cloned()
    }

    pub fn resolve_path(
        &mut self,
        segments: &[String],
        context: &Context,
        position: Option<Position>,
    ) -> Option<SymbolKey> {
        let Some(first) = segments.first() else {
            self.error(context.source, position, "empty symbol path");
            return None;
        };
        for key in self.path_candidates(first, segments, context) {
            if !self.graph.declarations.contains_key(&key) {
                continue;
            }
            if let Some(hidden) = self.hidden_boundary(&key, context) {
                self.error(
                    context.source,
                    position,
                    format!("module `{}` is private", hidden.source_name()),
                );
                return None;
            }
            if self.symbol_visible(&key, context) {
                return Some(key);
            }
            self.error(
                context.source,
                position,
                format!("declaration `{}` is private", key.source_name()),
            );
            return None;
        }
        self.error(
            context.source,
            position,
            format!("unknown symbol path `{}`", segments.join("::")),
        );
        None
    }

    pub fn rewrite_value_name(&mut self, name: &mut String, context: &Context, required: bool) {
        let key = if name.contains("::") {
            let segments = name.split("::").map(str::to_string).collect::<Vec<_>>();
            self.resolve_path(&segments, context, None)
        } else {
            self.resolve_bare(name, context, None)
        };
        if let Some(key) = key {
            *name = self.link_name(&key, false);
        } else if required && name.contains("::") {
            self.error(
                context.source,
                None,
                format!("unknown constructor `{name}`"),
            );
        }
    }

    pub(crate) fn rewrite_effect_name(&mut self, name: &mut String, context: &Context) {
        let qualified = name.contains("::");
        self.rewrite_value_name(name, context, qualified);
    }

    fn path_candidates(
        &self,
        first: &str,
        segments: &[String],
        context: &Context,
    ) -> Vec<SymbolKey> {
        let tail = segments.get(1..).unwrap_or_default();
        let scope = self.scopes.get(context.contribution);
        let mut candidates = Vec::new();
        for depth in (0..=context.module.len()).rev() {
            let mut path = context.module.get(..depth).unwrap_or_default().to_vec();
            path.extend_from_slice(segments);
            candidates.push(SymbolKey::new(context.namespace.clone(), path));
        }
        if let Some(base) = scope.and_then(|scope| scope.alias(first)) {
            candidates.push(extend_key(base, tail));
        }
        if let Some(base) = scope.and_then(|scope| scope.member(first)) {
            candidates.push(extend_key(base, tail));
        }
        if self.graph.namespaces.contains(first) {
            candidates.push(SymbolKey::new(first, tail.to_vec()));
        }
        candidates
    }

    fn symbol_visible(&self, key: &SymbolKey, context: &Context) -> bool {
        self.graph.declarations.get(key).is_some_and(|declaration| {
            declaration.visible_from(key.namespace == context.namespace, &context.module)
        })
    }

    fn hidden_boundary(&self, key: &SymbolKey, context: &Context) -> Option<SymbolKey> {
        (1..key.path.len()).find_map(|length| {
            let path = key.path.get(..length).unwrap_or_default().to_vec();
            let boundary = SymbolKey::new(key.namespace.clone(), path);
            self.graph
                .modules
                .get(&boundary)
                .and_then(|_| (!self.symbol_visible(&boundary, context)).then_some(boundary))
        })
    }
}

fn extend_key(base: &SymbolKey, tail: &[String]) -> SymbolKey {
    let mut path = base.path.clone();
    path.extend_from_slice(tail);
    SymbolKey::new(base.namespace.clone(), path)
}
