//! Declaration and signature collection for the logical project graph.
//! Implements [MODULES-EXPORTS], [MODULES-SIGNATURE], and
//! [MODULES-STATE-TOPLEVEL].

use crate::collect_support::primary_name;
use crate::contribution::Contribution;
use crate::model::{ConstantInfo, DeclInfo, DeclKind, ModuleInfo, ProjectGraph, SymbolKey};
use crate::{ProjectError, SourceMetadata};
use osprey_ast::{ModuleItem, ModuleKind, Position, SignatureItem, Stmt, Visibility};
use std::collections::BTreeSet;

pub(crate) fn collect(
    contributions: &[Contribution],
    sources: &[SourceMetadata],
) -> (ProjectGraph, Vec<ProjectError>) {
    let mut collector = Collector {
        graph: ProjectGraph::default(),
        errors: Vec::new(),
        sources,
    };
    for (index, contribution) in contributions.iter().enumerate() {
        let namespace = contribution.namespace.label();
        let _ = collector.graph.namespaces.insert(namespace.to_string());
        for statement in &contribution.statements {
            collector.statement(
                statement,
                namespace,
                &[],
                Visibility::Exported,
                false,
                contribution.source,
                index,
            );
        }
    }
    collector.finish_signatures();
    (collector.graph, collector.errors)
}

pub(crate) struct Collector<'a> {
    pub(crate) graph: ProjectGraph,
    pub(crate) errors: Vec<ProjectError>,
    pub(crate) sources: &'a [SourceMetadata],
}

impl Collector<'_> {
    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "exhaustive declaration collection keeps AST cases visibly adjacent"
    )]
    fn statement(
        &mut self,
        statement: &Stmt,
        namespace: &str,
        owner: &[String],
        visibility: Visibility,
        opaque: bool,
        source: usize,
        contribution: usize,
    ) {
        match statement {
            Stmt::Module {
                path,
                kind,
                signature,
                body,
                position,
                ..
            } => self.module(
                namespace,
                owner,
                path.segments.as_slice(),
                *kind,
                signature.clone(),
                body,
                visibility,
                source,
                contribution,
                *position,
            ),
            Stmt::Signature {
                name,
                items,
                position,
                ..
            } => self.signature(namespace, owner, name, items, source, *position),
            Stmt::Let {
                name,
                mutable,
                value,
                position,
                ..
            } => {
                if *mutable && owner.is_empty() {
                    self.error(source, *position, "namespace-level `mut` is forbidden");
                }
                let key = key(namespace, owner, name);
                self.declare(
                    key.clone(),
                    DeclKind::Value,
                    visibility,
                    owner,
                    source,
                    *position,
                    *mutable,
                    opaque,
                );
                let _ = self
                    .graph
                    .implementations
                    .entry(key.clone())
                    .or_insert_with(|| statement.clone());
                if !mutable {
                    let _ = self.graph.constants.insert(
                        key,
                        ConstantInfo {
                            value: value.clone(),
                            source,
                            contribution,
                            module: owner.to_vec(),
                            position: *position,
                        },
                    );
                }
            }
            Stmt::Function { name, position, .. } => {
                let key = key(namespace, owner, name);
                self.declare(
                    key.clone(),
                    DeclKind::Function,
                    visibility,
                    owner,
                    source,
                    *position,
                    false,
                    opaque,
                );
                let _ = self
                    .graph
                    .implementations
                    .entry(key)
                    .or_insert_with(|| statement.clone());
            }
            Stmt::Extern { name, position, .. } => {
                let key = key(namespace, owner, name);
                self.declare(
                    key.clone(),
                    DeclKind::Extern,
                    visibility,
                    owner,
                    source,
                    *position,
                    false,
                    opaque,
                );
                let _ = self
                    .graph
                    .implementations
                    .entry(key)
                    .or_insert_with(|| statement.clone());
            }
            Stmt::Type {
                name,
                variants,
                position,
                ..
            } => {
                let type_key = key(namespace, owner, name);
                self.declare(
                    type_key.clone(),
                    DeclKind::Type,
                    visibility,
                    owner,
                    source,
                    *position,
                    false,
                    opaque,
                );
                let _ = self
                    .graph
                    .implementations
                    .entry(type_key)
                    .or_insert_with(|| statement.clone());
                for variant in variants.iter().filter(|variant| variant.name != *name) {
                    self.declare(
                        key(namespace, owner, &variant.name),
                        DeclKind::Constructor,
                        if opaque {
                            Visibility::Private
                        } else {
                            visibility
                        },
                        owner,
                        source,
                        *position,
                        false,
                        opaque,
                    );
                }
            }
            Stmt::Effect { name, position, .. } => {
                let key = key(namespace, owner, name);
                self.declare(
                    key.clone(),
                    DeclKind::Effect,
                    visibility,
                    owner,
                    source,
                    *position,
                    false,
                    opaque,
                );
                let _ = self
                    .graph
                    .implementations
                    .entry(key)
                    .or_insert_with(|| statement.clone());
            }
            Stmt::Import(_)
            | Stmt::Namespace { .. }
            | Stmt::Assignment { .. }
            | Stmt::Expr { .. } => {}
        }
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "module metadata is supplied directly from the canonical AST node"
    )]
    fn module(
        &mut self,
        namespace: &str,
        owner: &[String],
        path: &[String],
        kind: ModuleKind,
        signature: Option<osprey_ast::SignatureAscription>,
        body: &[ModuleItem],
        visibility: Visibility,
        source: usize,
        contribution: usize,
        position: Option<Position>,
    ) {
        let mut full = owner.to_vec();
        full.extend_from_slice(path);
        let module_key = SymbolKey::new(namespace, full.clone());
        if kind == ModuleKind::State
            && self.graph.modules.iter().any(|(existing, info)| {
                existing.namespace == namespace && info.kind == ModuleKind::State
            })
        {
            self.error(
                source,
                position,
                format!("namespace `{namespace}` may contain at most one state module"),
            );
        }
        self.declare(
            module_key.clone(),
            DeclKind::Module,
            visibility,
            owner,
            source,
            position,
            kind == ModuleKind::State,
            false,
        );
        let _ = self
            .graph
            .implementations
            .entry(module_key.clone())
            .or_insert_with(|| Stmt::Module {
                path: osprey_ast::SymbolPath::new(path.iter().cloned()),
                kind,
                signature: signature.clone(),
                body: body.to_vec(),
                doc: None,
                position,
            });
        let explicit_exports = body
            .iter()
            .filter(|item| item.visibility == Visibility::Exported)
            .filter_map(|item| primary_name(&item.declaration))
            .collect::<BTreeSet<_>>();
        let mut state_cells = BTreeSet::new();
        let mut effects = BTreeSet::new();
        for item in body {
            if let Stmt::Let {
                name,
                mutable: true,
                position,
                ..
            } = item.declaration.as_ref()
            {
                if kind == ModuleKind::Plain {
                    self.error(
                        source,
                        *position,
                        "plain modules cannot declare module-level `mut`",
                    );
                } else {
                    let _ = state_cells.insert(name.clone());
                    if item.visibility == Visibility::Exported {
                        self.error(
                            source,
                            *position,
                            "`export mut` is forbidden in a state module",
                        );
                    }
                }
            }
            if let Stmt::Effect { name, .. } = item.declaration.as_ref() {
                let _ = effects.insert(name.clone());
            }
            self.statement(
                &item.declaration,
                namespace,
                &full,
                item.visibility,
                item.opaque,
                source,
                contribution,
            );
        }
        let info = ModuleInfo {
            kind,
            signature,
            exports: explicit_exports.clone(),
            explicit_exports,
            state_cells,
            effects,
            source,
            position,
        };
        if self
            .graph
            .modules
            .insert(module_key.clone(), info)
            .is_some()
        {
            self.error(
                source,
                position,
                format!("duplicate module `{}`", module_key.source_name()),
            );
        }
    }

    fn signature(
        &mut self,
        namespace: &str,
        owner: &[String],
        name: &str,
        items: &[SignatureItem],
        source: usize,
        position: Option<Position>,
    ) {
        let signature = key(namespace, owner, name);
        if self
            .graph
            .signatures
            .insert(signature.clone(), items.to_vec())
            .is_some()
        {
            self.error(
                source,
                position,
                format!("duplicate signature `{}`", signature.source_name()),
            );
        }
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::needless_pass_by_value,
        reason = "declaration registration owns its stable graph key and source metadata"
    )]
    fn declare(
        &mut self,
        key: SymbolKey,
        kind: DeclKind,
        visibility: Visibility,
        owner: &[String],
        source: usize,
        position: Option<Position>,
        state_owner: bool,
        opaque: bool,
    ) {
        let info = DeclInfo {
            kind,
            visibility,
            owner: owner.to_vec(),
            state_owner,
            opaque,
        };
        if self.graph.declarations.insert(key.clone(), info).is_some() {
            self.error(
                source,
                position,
                format!("duplicate declaration `{}`", key.source_name()),
            );
        }
    }
}

fn key(namespace: &str, owner: &[String], name: &str) -> SymbolKey {
    let mut path = owner.to_vec();
    path.push(name.to_string());
    SymbolKey::new(namespace, path)
}
