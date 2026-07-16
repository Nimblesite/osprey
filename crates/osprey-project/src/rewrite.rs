//! Lexically aware value, declaration, and constructor rewriting.

use crate::model::SymbolKey;
use crate::purity::is_pure;
use crate::resolve::{symbol_key, Context, Locals, Resolver};
use osprey_ast::{Expr, MatchArm, Pattern, Stmt, TypeParam};

fn locals_with_type_parameters(type_params: &[TypeParam]) -> Locals {
    Locals {
        types: type_params
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect(),
        ..Locals::default()
    }
}

impl Resolver<'_> {
    pub fn constant_value(&mut self, key: &SymbolKey) -> Option<Expr> {
        if let Some(value) = self.constant_cache.get(key) {
            return Some(value.clone());
        }
        if self.invalid_constants.contains(key) {
            return None;
        }
        if !self.constant_active.insert(key.clone()) {
            let source = self
                .graph
                .constants
                .get(key)
                .map_or(0, |constant| constant.source);
            let position = self
                .graph
                .constants
                .get(key)
                .and_then(|constant| constant.position);
            self.error(
                source,
                position,
                format!(
                    "constant initializer cycle involving `{}`",
                    key.source_name()
                ),
            );
            let _ = self.invalid_constants.insert(key.clone());
            return None;
        }
        let Some(constant) = self.graph.constants.get(key).cloned() else {
            let _ = self.constant_active.remove(key);
            return None;
        };
        let context = Context {
            contribution: constant.contribution,
            source: constant.source,
            namespace: key.namespace.clone(),
            module: constant.module,
        };
        let mut value = constant.value;
        self.rewrite_expr(&mut value, &context, &mut Locals::default());
        let _ = self.constant_active.remove(key);
        if is_pure(&value) && !self.invalid_constants.contains(key) {
            let _ = self.constant_cache.insert(key.clone(), value.clone());
            Some(value)
        } else {
            None
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "declaration rewriting is an exhaustive canonical AST match"
    )]
    pub fn rewrite_declaration(
        &mut self,
        statement: &mut Stmt,
        context: &Context,
        opaque: bool,
        entry_main: bool,
    ) {
        let original_name = crate::annotation::declaration_name(statement).map(str::to_string);
        let annotation = original_name
            .as_deref()
            .and_then(|name| self.graph.annotations.get(&symbol_key(context, name)))
            .cloned();
        if let Some(annotation) = &annotation {
            self.apply_annotation(statement, annotation, context);
        }
        match statement {
            Stmt::Function {
                name,
                type_params,
                parameters,
                return_type,
                effects,
                body,
                ..
            } => {
                let key = symbol_key(context, name);
                let mut locals = locals_with_type_parameters(type_params);
                locals
                    .values
                    .extend(parameters.iter().map(|parameter| parameter.name.clone()));
                if entry_main {
                    locals
                        .values
                        .extend(self.entry_runtime_names.iter().cloned());
                }
                for parameter in parameters {
                    if let Some(ty) = &mut parameter.ty {
                        self.rewrite_type(ty, context, &mut locals);
                    }
                }
                if let Some(ty) = return_type {
                    self.rewrite_type(ty, context, &mut locals);
                }
                self.rewrite_effect_refs(effects, context, &mut locals);
                self.rewrite_expr(body, context, &mut locals);
                *name = self.link_name(&key, entry_main);
            }
            Stmt::Extern {
                name,
                parameters,
                return_type,
                ..
            } => {
                let key = symbol_key(context, name);
                let mut locals = Locals::default();
                for parameter in parameters {
                    self.rewrite_type(&mut parameter.ty, context, &mut locals);
                }
                if let Some(ty) = return_type {
                    self.rewrite_type(ty, context, &mut locals);
                }
                *name = self.link_name(&key, false);
            }
            Stmt::Type {
                name,
                type_params,
                variants,
                alias,
                validation_func,
                ..
            } => {
                let source_name = name.clone();
                let key = symbol_key(context, &source_name);
                let type_link = self.link_name(&key, false);
                let mut locals = locals_with_type_parameters(type_params);
                if let Some(representation) = alias {
                    self.rewrite_type(representation, context, &mut locals);
                    if opaque {
                        *alias = None;
                    }
                }
                for variant in variants {
                    let variant_name = variant.name.clone();
                    if variant_name == source_name {
                        variant.name.clone_from(&type_link);
                    } else {
                        let constructor = symbol_key(context, &variant_name);
                        variant.name = self.link_name(&constructor, false);
                    }
                    let saved = locals.values.clone();
                    locals
                        .values
                        .extend(variant.fields.iter().map(|field| field.name.clone()));
                    for field in &mut variant.fields {
                        field.ty = self.rewrite_type_text(&field.ty, context, &mut locals);
                        if let Some(constraint) = &mut field.constraint {
                            self.rewrite_expr(constraint, context, &mut locals);
                        }
                    }
                    locals.values = saved;
                }
                if let Some(validation) = validation_func {
                    self.rewrite_value_name(validation, context, false);
                }
                *name = type_link;
            }
            Stmt::Effect {
                name,
                type_params,
                operations,
                ..
            } => {
                let key = symbol_key(context, name);
                let mut locals = locals_with_type_parameters(type_params);
                for operation in operations {
                    operation.ty = self.rewrite_type_text(&operation.ty, context, &mut locals);
                    for parameter in &mut operation.parameters {
                        if let Some(ty) = &mut parameter.ty {
                            self.rewrite_type(ty, context, &mut locals);
                        }
                    }
                    operation.return_type =
                        self.rewrite_type_text(&operation.return_type, context, &mut locals);
                }
                *name = self.link_name(&key, false);
            }
            _ => {}
        }
    }

    pub fn rewrite_local_statement(
        &mut self,
        statement: &mut Stmt,
        context: &Context,
        locals: &mut Locals,
    ) {
        match statement {
            Stmt::Let {
                name, ty, value, ..
            } => {
                if let Some(ty) = ty {
                    self.rewrite_type(ty, context, locals);
                }
                self.rewrite_expr(value, context, locals);
                let _ = locals.values.insert(name.clone());
            }
            Stmt::Assignment {
                name,
                value,
                position,
            } => {
                if !locals.values.contains(name)
                    && self.resolve_bare(name, context, *position).is_some()
                {
                    self.error(
                        context.source,
                        *position,
                        format!("cannot assign to module declaration `{name}`"),
                    );
                }
                self.rewrite_expr(value, context, locals);
            }
            Stmt::Expr { value, .. } => self.rewrite_expr(value, context, locals),
            _ => {}
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "expression rewriting is an exhaustive canonical AST walk"
    )]
    pub fn rewrite_expr(&mut self, expression: &mut Expr, context: &Context, locals: &mut Locals) {
        if let Expr::Identifier(name) = expression {
            let name = name.clone();
            let mut rewritten = Expr::Identifier(name.clone());
            self.rewrite_identifier(&mut rewritten, &name, context, locals);
            *expression = rewritten;
            return;
        }
        match expression {
            Expr::Path(path) => {
                let segments = path.segments.clone();
                self.rewrite_path_expression(expression, &segments, context, locals);
            }
            Expr::InterpolatedStr(parts) => {
                for part in parts {
                    if let osprey_ast::InterpolatedPart::Expr(value) = part {
                        self.rewrite_expr(value, context, locals);
                    }
                }
            }
            Expr::List(items) => self.rewrite_exprs(items, context, locals),
            Expr::Map(entries) => {
                for entry in entries {
                    self.rewrite_expr(&mut entry.key, context, locals);
                    self.rewrite_expr(&mut entry.value, context, locals);
                }
            }
            Expr::Object(fields) | Expr::TypeConstructor { fields, .. } => {
                for field in fields {
                    self.rewrite_expr(&mut field.value, context, locals);
                }
                if let Expr::TypeConstructor {
                    name, type_args, ..
                } = expression
                {
                    self.rewrite_value_name(name, context, true);
                    for argument in type_args {
                        self.rewrite_type(argument, context, locals);
                    }
                }
            }
            Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
                self.rewrite_expr(left, context, locals);
                self.rewrite_expr(right, context, locals);
            }
            Expr::Unary { operand, .. }
            | Expr::Spawn(operand)
            | Expr::Await(operand)
            | Expr::Recv(operand) => self.rewrite_expr(operand, context, locals),
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                self.rewrite_expr(function, context, locals);
                self.rewrite_exprs(arguments, context, locals);
                for argument in named_arguments {
                    self.rewrite_expr(&mut argument.value, context, locals);
                }
            }
            Expr::MethodCall {
                target,
                method,
                arguments,
                named_arguments,
            } => {
                self.rewrite_expr(target, context, locals);
                self.rewrite_value_name(method, context, false);
                self.rewrite_exprs(arguments, context, locals);
                for argument in named_arguments {
                    self.rewrite_expr(&mut argument.value, context, locals);
                }
            }
            Expr::FieldAccess { target, .. } => self.rewrite_expr(target, context, locals),
            Expr::Index { target, index } => {
                self.rewrite_expr(target, context, locals);
                self.rewrite_expr(index, context, locals);
            }
            Expr::Lambda {
                parameters,
                return_type,
                body,
                ..
            } => {
                let saved = locals.clone();
                for parameter in parameters {
                    if let Some(ty) = &mut parameter.ty {
                        self.rewrite_type(ty, context, locals);
                    }
                    let _ = locals.values.insert(parameter.name.clone());
                }
                if let Some(ty) = return_type {
                    self.rewrite_type(ty, context, locals);
                }
                self.rewrite_expr(body, context, locals);
                *locals = saved;
            }
            Expr::Match { value, arms } => {
                self.rewrite_expr(value, context, locals);
                self.rewrite_pattern_arms(arms, context, locals);
            }
            Expr::Block { statements, value } => {
                let saved = locals.clone();
                for statement in statements {
                    self.rewrite_local_statement(statement, context, locals);
                }
                if let Some(value) = value {
                    self.rewrite_expr(value, context, locals);
                }
                *locals = saved;
            }
            Expr::Update { record, fields } => {
                if !locals.values.contains(record) {
                    self.rewrite_value_name(record, context, false);
                }
                for field in fields {
                    self.rewrite_expr(&mut field.value, context, locals);
                }
            }
            Expr::Yield(value) | Expr::Resume(value) => {
                if let Some(value) = value {
                    self.rewrite_expr(value, context, locals);
                }
            }
            Expr::Send { channel, value } => {
                self.rewrite_expr(channel, context, locals);
                self.rewrite_expr(value, context, locals);
            }
            Expr::Select { arms } => self.rewrite_pattern_arms(arms, context, locals),
            Expr::Perform {
                effect,
                arguments,
                named_arguments,
                ..
            } => {
                self.rewrite_effect_name(effect, context);
                self.rewrite_exprs(arguments, context, locals);
                for argument in named_arguments {
                    self.rewrite_expr(&mut argument.value, context, locals);
                }
            }
            Expr::Handler {
                effect, arms, body, ..
            } => {
                self.rewrite_effect_name(effect, context);
                self.rewrite_expr(body, context, locals);
                for arm in arms {
                    let saved = locals.clone();
                    locals.values.extend(arm.params.iter().cloned());
                    self.rewrite_expr(&mut arm.body, context, locals);
                    *locals = saved;
                }
            }
            Expr::Identifier(_)
            | Expr::Integer(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Bool(_) => {}
        }
    }

    fn rewrite_identifier(
        &mut self,
        expression: &mut Expr,
        name: &str,
        context: &Context,
        locals: &Locals,
    ) {
        if locals.values.contains(name) {
            return;
        }
        let Some(key) = self.resolve_bare(name, context, None) else {
            return;
        };
        self.replace_symbol_expression(expression, &key, context);
    }

    fn rewrite_path_expression(
        &mut self,
        expression: &mut Expr,
        segments: &[String],
        context: &Context,
        _locals: &Locals,
    ) {
        if let Some(key) = self.resolve_path(segments, context, None) {
            self.replace_symbol_expression(expression, &key, context);
        }
    }

    fn rewrite_pattern_arms(
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
                self.rewrite_value_name(name, context, true);
                locals.values.extend(fields.iter().cloned());
                for nested in sub_patterns {
                    self.rewrite_pattern(nested, context, locals);
                }
            }
            Pattern::TypeAnnotated { name, ty } => {
                self.rewrite_type(ty, context, locals);
                let _ = locals.values.insert(name.clone());
            }
            Pattern::Structural { fields } => locals.values.extend(fields.iter().cloned()),
            Pattern::List { elements, rest } => {
                for nested in elements {
                    self.rewrite_pattern(nested, context, locals);
                }
                locals.values.extend(rest.iter().cloned());
            }
            Pattern::Binding(name) => locals.values.extend(std::iter::once(name.clone())),
        }
    }
}
