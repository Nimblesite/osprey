//! Type/effect-name rewriting and alias expansion.

use crate::model::{DeclKind, SymbolKey};
use crate::resolve::{Context, Locals, Resolver};
use osprey_ast::{EffectRef, TypeExpr};
use std::collections::BTreeMap;

impl Resolver<'_> {
    pub fn rewrite_type(&mut self, ty: &mut TypeExpr, context: &Context, locals: &mut Locals) {
        for parameter in &mut ty.generic_params {
            self.rewrite_type(parameter, context, locals);
        }
        if let Some(element) = &mut ty.array_element {
            self.rewrite_type(element, context, locals);
        }
        for parameter in &mut ty.parameter_types {
            self.rewrite_type(parameter, context, locals);
        }
        if let Some(result) = &mut ty.return_type {
            self.rewrite_type(result, context, locals);
        }
        if ty.name.is_empty() || locals.types.contains(&ty.name) {
            return;
        }
        let Some(key) = self.resolve_type_key(&ty.name, context, ty.position) else {
            return;
        };
        let Some(declaration) = self.graph.declarations.get(&key) else {
            return;
        };
        if declaration.kind != DeclKind::Type {
            self.error(
                context.source,
                ty.position,
                format!("`{}` does not name a type", key.source_name()),
            );
            return;
        }
        let alias = self.aliases.get(&key).cloned();
        if let Some(alias) = alias {
            let visible_representation = !alias.opaque || context.module.starts_with(&alias.owner);
            if visible_representation {
                self.expand_alias(ty, &key, &alias, context);
                return;
            }
        }
        ty.name = self.link_name(&key, false);
    }

    pub fn rewrite_effect_refs(
        &mut self,
        effects: &mut [EffectRef],
        context: &Context,
        locals: &mut Locals,
    ) {
        for effect in effects {
            for argument in &mut effect.type_args {
                self.rewrite_type(argument, context, locals);
            }
            let original = effect.name.clone();
            let key = if original.contains("::") {
                let path = original.split("::").map(str::to_string).collect::<Vec<_>>();
                self.resolve_path(&path, context, effect.position)
            } else {
                self.resolve_bare(&original, context, effect.position)
            };
            if let Some(key) = key {
                if self
                    .graph
                    .declarations
                    .get(&key)
                    .is_some_and(|declaration| declaration.kind == DeclKind::Effect)
                {
                    effect.name = self.link_name(&key, false);
                } else {
                    self.error(
                        context.source,
                        effect.position,
                        format!("`{}` does not name an effect", key.source_name()),
                    );
                }
            }
        }
    }

    pub fn rewrite_type_text(
        &mut self,
        text: &str,
        context: &Context,
        locals: &mut Locals,
    ) -> String {
        let mut output = String::with_capacity(text.len());
        let mut token = String::new();
        for character in text.chars().chain(std::iter::once('\0')) {
            if character.is_alphanumeric() || character == '_' || character == ':' {
                token.push(character);
                continue;
            }
            if !token.is_empty() {
                output.push_str(&self.rewrite_type_token(&token, context, locals));
                token.clear();
            }
            if character != '\0' {
                output.push(character);
            }
        }
        output
    }

    fn rewrite_type_token(
        &mut self,
        token: &str,
        context: &Context,
        locals: &mut Locals,
    ) -> String {
        if token
            .chars()
            .next()
            .is_none_or(|first| first.is_ascii_digit())
        {
            return token.to_string();
        }
        let mut ty = TypeExpr::named(token);
        self.rewrite_type(&mut ty, context, locals);
        render_type(&ty)
    }

    fn resolve_type_key(
        &mut self,
        name: &str,
        context: &Context,
        position: Option<osprey_ast::Position>,
    ) -> Option<SymbolKey> {
        if name.contains("::") {
            let path = name.split("::").map(str::to_string).collect::<Vec<_>>();
            self.resolve_path(&path, context, position)
        } else {
            self.resolve_bare(name, context, position)
        }
    }

    fn expand_alias(
        &mut self,
        target: &mut TypeExpr,
        key: &SymbolKey,
        alias: &crate::resolve::AliasInfo,
        use_context: &Context,
    ) {
        if !self.alias_active.insert(key.clone()) {
            self.error(
                use_context.source,
                target.position,
                format!("type alias cycle involving `{}`", key.source_name()),
            );
            return;
        }
        if alias.type_params.len() != target.generic_params.len() {
            self.error(
                use_context.source,
                target.position,
                format!(
                    "type alias `{}` expects {} type argument(s), found {}",
                    key.source_name(),
                    alias.type_params.len(),
                    target.generic_params.len()
                ),
            );
            let _ = self.alias_active.remove(key);
            return;
        }
        let substitutions = alias
            .type_params
            .iter()
            .cloned()
            .zip(target.generic_params.iter().cloned())
            .collect::<BTreeMap<_, _>>();
        let mut representation = alias.value.clone();
        substitute(&mut representation, &substitutions);
        let definition_context = Context {
            contribution: alias.contribution,
            source: alias.source,
            namespace: key.namespace.clone(),
            module: alias.owner.clone(),
        };
        self.rewrite_type(
            &mut representation,
            &definition_context,
            &mut Locals::default(),
        );
        let _ = self.alias_active.remove(key);
        *target = representation;
    }
}

fn substitute(ty: &mut TypeExpr, substitutions: &BTreeMap<String, TypeExpr>) {
    if let Some(replacement) = substitutions.get(&ty.name) {
        *ty = replacement.clone();
        return;
    }
    for parameter in &mut ty.generic_params {
        substitute(parameter, substitutions);
    }
    if let Some(element) = &mut ty.array_element {
        substitute(element, substitutions);
    }
    for parameter in &mut ty.parameter_types {
        substitute(parameter, substitutions);
    }
    if let Some(result) = &mut ty.return_type {
        substitute(result, substitutions);
    }
}

fn render_type(ty: &TypeExpr) -> String {
    if ty.is_array {
        return ty.array_element.as_deref().map_or_else(
            || "[]".to_string(),
            |element| format!("[{}]", render_type(element)),
        );
    }
    if ty.is_function {
        let parameters = ty
            .parameter_types
            .iter()
            .map(render_type)
            .collect::<Vec<_>>()
            .join(", ");
        let result = ty
            .return_type
            .as_deref()
            .map_or_else(|| "unit".to_string(), render_type);
        return format!("fn({parameters}) -> {result}");
    }
    if ty.generic_params.is_empty() {
        ty.name.clone()
    } else {
        let parameters = ty
            .generic_params
            .iter()
            .map(render_type)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}<{parameters}>", ty.name)
    }
}
