//! Project flattening and logical-name resolution.
//! Implements [MODULES-ABI], [MODULES-ENTRYPOINT], and [MODULES-INIT].

use crate::assemble::source_error;
use crate::contribution::Contribution;
use crate::imports::ImportScope;
use crate::model::{DeclKind, ProjectGraph, SymbolKey};
use crate::purity::is_pure;
use crate::{state, ProjectError, SourceMetadata};
use osprey_ast::{Expr, ModuleItem, ModuleKind, Position, Program, Stmt, TypeExpr};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Resolution {
    pub program: Program,
    pub entry_prologue: Vec<Stmt>,
    pub source_names: BTreeMap<String, String>,
    pub errors: Vec<ProjectError>,
}

#[derive(Debug, Clone)]
pub(crate) struct Context {
    pub contribution: usize,
    pub source: usize,
    pub namespace: String,
    pub module: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AliasInfo {
    pub type_params: Vec<String>,
    pub value: TypeExpr,
    pub opaque: bool,
    pub owner: Vec<String>,
    pub source: usize,
    pub contribution: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Locals {
    pub values: BTreeSet<String>,
    pub types: BTreeSet<String>,
}

pub(crate) struct Resolver<'a> {
    pub graph: &'a ProjectGraph,
    pub scopes: &'a [ImportScope],
    pub sources: &'a [SourceMetadata],
    pub entry_source: usize,
    pub errors: Vec<ProjectError>,
    pub source_names: BTreeMap<String, String>,
    pub aliases: BTreeMap<SymbolKey, AliasInfo>,
    pub alias_active: BTreeSet<SymbolKey>,
    pub constant_cache: BTreeMap<SymbolKey, Expr>,
    pub constant_active: BTreeSet<SymbolKey>,
    pub invalid_constants: BTreeSet<SymbolKey>,
    pub runtime_constants: BTreeSet<SymbolKey>,
    pub entry_runtime_names: BTreeSet<String>,
    pub(crate) program: Vec<Stmt>,
    entry_prologue: Vec<Stmt>,
    entry_locals: Locals,
    entry_main_count: usize,
    extern_names: BTreeMap<String, SymbolKey>,
}

pub(crate) fn flatten(
    contributions: &[Contribution],
    scopes: &[ImportScope],
    graph: &ProjectGraph,
    entry_source: usize,
    sources: &[SourceMetadata],
) -> Resolution {
    let aliases = collect_aliases(contributions, graph);
    let mut resolver = Resolver {
        graph,
        scopes,
        sources,
        entry_source,
        errors: Vec::new(),
        source_names: BTreeMap::new(),
        aliases,
        alias_active: BTreeSet::new(),
        constant_cache: BTreeMap::new(),
        constant_active: BTreeSet::new(),
        invalid_constants: BTreeSet::new(),
        runtime_constants: BTreeSet::new(),
        entry_runtime_names: BTreeSet::new(),
        program: Vec::new(),
        entry_prologue: Vec::new(),
        entry_locals: Locals::default(),
        entry_main_count: 0,
        extern_names: BTreeMap::new(),
    };
    resolver.classify_constants();
    for (index, contribution) in contributions.iter().enumerate() {
        let context = Context {
            contribution: index,
            source: contribution.source,
            namespace: contribution.namespace.label().to_string(),
            module: Vec::new(),
        };
        for statement in &contribution.statements {
            resolver.flatten_statement(statement, &context, false);
        }
    }
    resolver.finish()
}

impl Resolver<'_> {
    fn finish(mut self) -> Resolution {
        if self.entry_main_count == 0 {
            self.program.extend(self.entry_prologue.clone());
        } else if self.entry_main_count == 1 {
            for statement in &mut self.program {
                if let Stmt::Function { name, body, .. } = statement {
                    if name == "main" {
                        inject_prologue(body, &self.entry_prologue);
                        break;
                    }
                }
            }
        }
        Resolution {
            program: Program {
                statements: self.program,
            },
            entry_prologue: self.entry_prologue,
            source_names: self.source_names,
            errors: self.errors,
        }
    }

    fn classify_constants(&mut self) {
        let keys = self.graph.constants.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            if self.constant_value(&key).is_some() {
                continue;
            }
            let Some(info) = self.graph.constants.get(&key) else {
                continue;
            };
            if info.source == self.entry_source
                && info.module.is_empty()
                && !self.invalid_constants.contains(&key)
            {
                let _ = self.runtime_constants.insert(key.clone());
                if let Some(name) = key.path.last() {
                    let _ = self.entry_runtime_names.insert(name.clone());
                }
            } else if !self.invalid_constants.contains(&key) {
                self.error(
                    info.source,
                    info.position,
                    format!(
                        "initializer for `{}` must be a compile-time constant",
                        key.source_name()
                    ),
                );
                let _ = self.invalid_constants.insert(key);
            }
        }
    }

    fn flatten_statement(&mut self, statement: &Stmt, context: &Context, opaque: bool) {
        match statement {
            Stmt::Module {
                path, kind, body, ..
            } => self.flatten_module(path.segments.as_slice(), *kind, body, context),
            Stmt::Let { name, .. } => self.flatten_let(statement, name, context),
            Stmt::Function {
                name,
                parameters,
                position,
                ..
            } => {
                let entry_main = context.source == self.entry_source
                    && context.module.is_empty()
                    && name == "main";
                if name == "main"
                    && context.module.is_empty()
                    && context.source != self.entry_source
                {
                    self.error(
                        context.source,
                        *position,
                        "namespace-level `main` is only allowed in the selected entry source",
                    );
                }
                if entry_main {
                    self.entry_main_count = self.entry_main_count.saturating_add(1);
                    if self.entry_main_count > 1 {
                        self.error(
                            context.source,
                            *position,
                            "the entry source declares more than one namespace-level `main`",
                        );
                    }
                    if !parameters.is_empty() {
                        self.error(
                            context.source,
                            *position,
                            "project entry function `main` cannot have parameters",
                        );
                    }
                }
                let mut rewritten = statement.clone();
                self.rewrite_declaration(&mut rewritten, context, opaque, entry_main);
                self.program.push(rewritten);
            }
            Stmt::Extern { name, position, .. } => {
                let key = symbol_key(context, name);
                if let Some(previous) = self.extern_names.get(name) {
                    if previous != &key {
                        self.error(
                            context.source,
                            *position,
                            format!("external symbol `{name}` is declared by multiple modules"),
                        );
                    }
                    return;
                }
                let _ = self.extern_names.insert(name.clone(), key);
                let mut rewritten = statement.clone();
                self.rewrite_declaration(&mut rewritten, context, opaque, false);
                self.program.push(rewritten);
            }
            Stmt::Type { alias: Some(_), .. } if !opaque => {
                let mut declaration = statement.clone();
                self.rewrite_declaration(&mut declaration, context, false, false);
            }
            Stmt::Type { alias: Some(_), .. } if opaque => {
                self.reject_opaque_alias(statement, context);
            }
            Stmt::Type { .. } | Stmt::Effect { .. } => {
                let mut rewritten = statement.clone();
                self.rewrite_declaration(&mut rewritten, context, opaque, false);
                self.program.push(rewritten);
            }
            Stmt::Assignment { position, .. } | Stmt::Expr { position, .. } => {
                if context.source == self.entry_source && context.module.is_empty() {
                    let mut rewritten = statement.clone();
                    self.rewrite_entry_statement(&mut rewritten, context);
                    self.entry_prologue.push(rewritten);
                } else {
                    self.error(
                        context.source,
                        *position,
                        "executable top-level statements are only allowed in the entry source",
                    );
                }
            }
            Stmt::Import(_) | Stmt::Namespace { .. } | Stmt::Signature { .. } => {}
        }
    }

    fn flatten_module(
        &mut self,
        path: &[String],
        kind: ModuleKind,
        items: &[ModuleItem],
        context: &Context,
    ) {
        let mut module_path = context.module.clone();
        module_path.extend_from_slice(path);
        let key = SymbolKey::new(context.namespace.clone(), module_path.clone());
        let mut body = items.to_vec();
        if kind == ModuleKind::State {
            self.validate_state_initializers(&body, context, &module_path);
        }
        state::prepare(
            &key,
            &mut body,
            self.graph,
            self.scopes.get(context.contribution),
            self.sources,
            &mut self.errors,
        );
        let nested = Context {
            contribution: context.contribution,
            source: context.source,
            namespace: context.namespace.clone(),
            module: module_path,
        };
        for item in &body {
            let graph_opaque = match item.declaration.as_ref() {
                Stmt::Type { name, .. } => self
                    .graph
                    .declarations
                    .get(&symbol_key(&nested, name))
                    .is_some_and(|declaration| declaration.opaque),
                _ => false,
            };
            self.flatten_statement(&item.declaration, &nested, item.opaque || graph_opaque);
        }
    }

    fn validate_state_initializers(
        &mut self,
        body: &[ModuleItem],
        context: &Context,
        module: &[String],
    ) {
        let nested = Context {
            contribution: context.contribution,
            source: context.source,
            namespace: context.namespace.clone(),
            module: module.to_vec(),
        };
        for item in body {
            if let Stmt::Let {
                name,
                mutable: true,
                value,
                position,
                ..
            } = item.declaration.as_ref()
            {
                let mut value = value.clone();
                self.rewrite_expr(&mut value, &nested, &mut Locals::default());
                if !is_pure(&value) {
                    self.error(
                        context.source,
                        *position,
                        format!(
                            "state initializer `{name}` must be pure and cannot depend on another state cell"
                        ),
                    );
                }
            }
        }
    }

    fn flatten_let(&mut self, statement: &Stmt, name: &str, context: &Context) {
        let key = symbol_key(context, name);
        if self.constant_cache.contains_key(&key) {
            return;
        }
        if self.runtime_constants.contains(&key) {
            let mut rewritten = statement.clone();
            self.rewrite_entry_statement(&mut rewritten, context);
            self.entry_prologue.push(rewritten);
            return;
        }
        if matches!(statement, Stmt::Let { mutable: true, .. })
            && context.source == self.entry_source
            && context.module.is_empty()
        {
            let mut rewritten = statement.clone();
            self.rewrite_entry_statement(&mut rewritten, context);
            self.entry_prologue.push(rewritten);
        }
    }

    fn rewrite_entry_statement(&mut self, statement: &mut Stmt, context: &Context) {
        let mut locals = std::mem::take(&mut self.entry_locals);
        self.rewrite_local_statement(statement, context, &mut locals);
        self.entry_locals = locals;
    }

    pub fn link_name(&mut self, key: &SymbolKey, entry_main: bool) -> String {
        let name = if entry_main {
            "main".to_string()
        } else if self
            .graph
            .declarations
            .get(key)
            .is_some_and(|declaration| declaration.kind == DeclKind::Extern)
        {
            key.path.last().cloned().unwrap_or_else(|| key.mangled())
        } else {
            key.mangled()
        };
        let _ = self.source_names.insert(name.clone(), key.source_name());
        name
    }

    pub fn error(&mut self, source: usize, position: Option<Position>, message: impl Into<String>) {
        self.errors
            .push(source_error(self.sources, source, position, message));
    }
}

fn collect_aliases(
    contributions: &[Contribution],
    graph: &ProjectGraph,
) -> BTreeMap<SymbolKey, AliasInfo> {
    let mut aliases = BTreeMap::new();
    for (index, contribution) in contributions.iter().enumerate() {
        collect_alias_statements(
            &contribution.statements,
            contribution.namespace.label(),
            &[],
            contribution.source,
            index,
            graph,
            &mut aliases,
        );
    }
    aliases
}

pub(crate) fn symbol_key(context: &Context, name: &str) -> SymbolKey {
    let mut path = context.module.clone();
    path.push(name.to_string());
    SymbolKey::new(context.namespace.clone(), path)
}

fn collect_alias_statements(
    statements: &[Stmt],
    namespace: &str,
    module: &[String],
    source: usize,
    contribution: usize,
    graph: &ProjectGraph,
    aliases: &mut BTreeMap<SymbolKey, AliasInfo>,
) {
    for statement in statements {
        match statement {
            Stmt::Type {
                name,
                type_params,
                alias: Some(value),
                ..
            } => {
                let mut path = module.to_vec();
                path.push(name.clone());
                let key = SymbolKey::new(namespace, path);
                let opaque = graph
                    .declarations
                    .get(&key)
                    .is_some_and(|declaration| declaration.opaque);
                let _ = aliases.insert(
                    key,
                    AliasInfo {
                        type_params: type_params
                            .iter()
                            .map(|parameter| parameter.name.clone())
                            .collect(),
                        value: value.clone(),
                        opaque,
                        owner: module.to_vec(),
                        source,
                        contribution,
                    },
                );
            }
            Stmt::Module { path, body, .. } => {
                let mut nested = module.to_vec();
                nested.extend_from_slice(&path.segments);
                let declarations = body
                    .iter()
                    .map(|item| item.declaration.as_ref().clone())
                    .collect::<Vec<_>>();
                collect_alias_statements(
                    &declarations,
                    namespace,
                    &nested,
                    source,
                    contribution,
                    graph,
                    aliases,
                );
            }
            _ => {}
        }
    }
}

fn inject_prologue(body: &mut Expr, prologue: &[Stmt]) {
    if prologue.is_empty() {
        return;
    }
    if let Expr::Block { statements, .. } = body {
        let mut combined = prologue.to_vec();
        combined.append(statements);
        *statements = combined;
    } else {
        let original = std::mem::replace(body, Expr::Bool(false));
        *body = Expr::Block {
            statements: prologue.to_vec(),
            value: Some(Box::new(original)),
        };
    }
}
