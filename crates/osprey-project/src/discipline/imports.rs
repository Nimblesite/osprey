//! The imported signatures of a checker request ([TYPES-DISCIPLINE-PROTOCOL]):
//! sources are visited in dependency order, each `hm` module an importer
//! needs has its export signature inferred by the ordinary checker and
//! rendered in ML signature syntax, each checked module contributes the
//! signature its checker wrote, and a request lists what its source imports.

use super::protocol::Import;
use super::HM;
use crate::{ProjectConfig, ProjectError, SourceFile};
use osprey_ast::{NamespaceName, Stmt};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The ML export signature of every module of `source`, for `osprey
/// <file> --signature`: what a checker of an importing module would receive
/// for it under `hm`, rendered by the same code.
///
/// # Errors
/// Assembly or inference of the file failed; the diagnostic names it.
pub fn export_signature(
    config: &ProjectConfig,
    source: &SourceFile,
) -> Result<String, ProjectError> {
    let mut exports = Exports::default();
    exports.record_hm(config, source, &[])?;
    Ok(exports
        .order
        .iter()
        .filter_map(|key| exports.signatures.get(key))
        .map(|import| import.signature.as_str())
        .collect::<Vec<_>>()
        .join(""))
}

/// Dependency order of `sources`: a file comes after every file it imports
/// a module or namespace from; files in an import cycle keep their given
/// order, which assembly reports as it does today.
#[must_use]
pub(super) fn order(sources: &[SourceFile]) -> Vec<usize> {
    let owners = owners(sources);
    let mut pending: Vec<usize> = (0..sources.len()).collect();
    let mut placed = Vec::with_capacity(sources.len());
    let mut done = BTreeSet::new();
    while let Some(first) = pending.first().copied() {
        let ready = next_ready(sources, &owners, &pending, &done).unwrap_or(first);
        pending.retain(|index| *index != ready);
        let _ = done.insert(ready);
        placed.push(ready);
    }
    placed
}

/// The first pending file whose imported files are all done.
fn next_ready(
    sources: &[SourceFile],
    owners: &HashMap<String, Vec<usize>>,
    pending: &[usize],
    done: &BTreeSet<usize>,
) -> Option<usize> {
    pending.iter().copied().find(|index| {
        sources.get(*index).is_some_and(|source| {
            imported_files(source, owners, *index)
                .iter()
                .all(|file| done.contains(file))
        })
    })
}

/// Which files declare each `ns` and each `ns::Module` key.
fn owners(sources: &[SourceFile]) -> HashMap<String, Vec<usize>> {
    let mut owners: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, source) in sources.iter().enumerate() {
        for key in declared_keys(source) {
            owners.entry(key).or_default().push(index);
        }
    }
    owners
}

/// `ns` for every namespace of the file and `ns::Module` for every module.
fn declared_keys(source: &SourceFile) -> Vec<String> {
    let mut keys = Vec::new();
    for statement in &source.program.statements {
        if let Stmt::Namespace { name, body, .. } = statement {
            keys.push(name.label().to_string());
            keys.extend(module_keys(name, body));
        }
    }
    keys
}

fn module_keys(namespace: &NamespaceName, body: &[Stmt]) -> Vec<String> {
    body.iter()
        .filter_map(|statement| match statement {
            Stmt::Module { path, .. } => Some(format!("{}::{path}", namespace.label())),
            _ => None,
        })
        .collect()
}

/// The `ns` or `ns::Module` keys a file imports, in source order.
fn import_keys(source: &SourceFile) -> Vec<String> {
    let mut keys = Vec::new();
    for statement in &source.program.statements {
        match statement {
            Stmt::Import(import) => keys.push(import_key(import)),
            Stmt::Namespace { body, .. } => {
                keys.extend(body.iter().filter_map(|inner| match inner {
                    Stmt::Import(import) => Some(import_key(import)),
                    _ => None,
                }));
            }
            _ => {}
        }
    }
    keys
}

fn import_key(import: &osprey_ast::ImportDecl) -> String {
    let namespace = import.target.namespace.label();
    if import.target.path.is_empty() {
        namespace.to_string()
    } else {
        format!("{namespace}::{}", import.target.path)
    }
}

/// The files some disciplined source imports from: the `hm` files whose
/// export signature a checker will receive, and nothing else is inferred.
#[must_use]
pub(super) fn needed(config: &ProjectConfig, sources: &[SourceFile]) -> BTreeSet<usize> {
    let owners = owners(sources);
    sources
        .iter()
        .enumerate()
        .filter(|(_, source)| super::select(config, &source.source) != HM)
        .flat_map(|(index, source)| imported_files(source, &owners, index))
        .collect()
}

/// The other files `source` imports from.
fn imported_files(
    source: &SourceFile,
    owners: &HashMap<String, Vec<usize>>,
    own: usize,
) -> BTreeSet<usize> {
    import_keys(source)
        .iter()
        .flat_map(|key| owners.get(key).into_iter().flatten().copied())
        .filter(|index| *index != own)
        .collect()
}

/// The export signatures gathered so far, keyed `ns::Module`, in the order
/// their files were checked, which is dependency order.
#[derive(Debug, Default)]
pub(super) struct Exports {
    signatures: BTreeMap<String, Import>,
    order: Vec<String>,
}

impl Exports {
    /// The signatures `source` imports, in dependency order: every module of
    /// an imported namespace, or the one module of a module import.
    #[must_use]
    pub(super) fn for_source(&self, source: &SourceFile) -> Vec<Import> {
        let wanted = import_keys(source);
        self.order
            .iter()
            .filter(|key| {
                wanted
                    .iter()
                    .any(|want| *key == want || key.starts_with(&format!("{want}::")))
            })
            .filter_map(|key| self.signatures.get(key).cloned())
            .collect()
    }

    /// Record the signature blocks a checker wrote for `source`.
    pub(super) fn record_checked(&mut self, source: &SourceFile, discipline: &str, text: &str) {
        let namespace = first_namespace(source);
        for (module, block) in blocks(text) {
            self.insert(
                format!("{namespace}::{module}"),
                discipline,
                block,
                Vec::new(),
            );
        }
    }

    /// Infer and record the export signature of every module of the `hm`
    /// file `source`, assembling it with the files already visited.
    ///
    /// # Errors
    /// Assembly or inference of the file failed; the diagnostic names it.
    pub(super) fn record_hm(
        &mut self,
        config: &ProjectConfig,
        source: &SourceFile,
        visited: &[SourceFile],
    ) -> Result<(), ProjectError> {
        let assembled = assemble_for(config, source, visited)?;
        let types = well_typed(source, &assembled)?;
        for (namespace, module, exports) in exported_functions(source) {
            let key = format!("{namespace}::{module}");
            let items = exports
                .iter()
                .filter_map(|name| item(&assembled, &types, &namespace, &module, name))
                .collect::<Vec<_>>();
            let signature = format!("signature {module}\n{}", items.join(""));
            self.insert(key, HM, signature, omitted_of(source, &module));
        }
        Ok(())
    }

    fn insert(&mut self, key: String, discipline: &str, signature: String, omitted: Vec<String>) {
        if !self.signatures.contains_key(&key) {
            self.order.push(key.clone());
        }
        let _ = self.signatures.insert(
            key.clone(),
            Import {
                name: key,
                discipline: discipline.to_string(),
                signature,
                omitted,
            },
        );
    }
}

/// The inferred types of a module that type-checks; the first type error
/// of one that does not, so no checker ever receives the signature of an
/// ill-typed import (`infer_program` alone answers through errors).
fn well_typed(
    source: &SourceFile,
    assembled: &crate::AssembledProject,
) -> Result<osprey_types::ProgramTypes, ProjectError> {
    match osprey_types::check_program(&assembled.program).first() {
        None => Ok(osprey_types::infer_program(&assembled.program)),
        Some(error) => Err(ProjectError {
            message: format!(
                "{}: the export signature of `{}` could not be inferred for an importing checker: {}",
                super::ERROR_PREFIX,
                source.path.display(),
                error.message
            ),
            path: Some(source.path.clone()),
            line: None,
            column: None,
        }),
    }
}

/// Assemble `source` as the entry together with `visited`, which holds every
/// file ordered before it and so everything it can import.
fn assemble_for(
    config: &ProjectConfig,
    source: &SourceFile,
    visited: &[SourceFile],
) -> Result<crate::AssembledProject, ProjectError> {
    let mut config = config.clone();
    config.entry = Some(source.path.clone());
    let mut subset = visited.to_vec();
    subset.push(source.clone());
    crate::assemble::assemble(&config, &subset).map_err(|errors| ProjectError {
        message: format!(
            "{}: the export signature of `{}` could not be inferred for an importing checker: {}",
            super::ERROR_PREFIX,
            source.path.display(),
            errors
                .first()
                .map(|error| error.message.clone())
                .unwrap_or_default()
        ),
        path: Some(source.path.clone()),
        line: None,
        column: None,
    })
}

/// One `    name : type` line for an exported function, from the inferred
/// types; `None` for an export that is not a function (a type or a value),
/// which the first version of the seam leaves out of the signature.
fn item(
    assembled: &crate::AssembledProject,
    types: &osprey_types::ProgramTypes,
    namespace: &str,
    module: &str,
    name: &str,
) -> Option<String> {
    let source_name = format!("{namespace}::{module}::{name}");
    if !assembled.public_api.contains_key(&source_name) {
        return None;
    }
    let mangled = osprey_ast::symbol::mangle([namespace, module, name]);
    let (params, ret) = types.functions.get(&mangled)?;
    let mut params = params.clone();
    let mut ret = ret.clone();
    let count = renumber(&mut params, &mut ret);
    let binders = type_binders(count);
    let params = params.iter().map(ToString::to_string).collect::<Vec<_>>();
    Some(format!(
        "    {name}{binders} : ({}) -> {ret}\n",
        params.join(", ")
    ))
}

/// `<t0, t1>` for the `count` type variables of a polymorphic export, so the
/// line parses as a generic signature value; nothing for a monomorphic one.
fn type_binders(count: usize) -> String {
    if count == 0 {
        return String::new();
    }
    let names = (0..count)
        .map(|index| format!("t{index}"))
        .collect::<Vec<_>>();
    format!("<{}>", names.join(", "))
}

/// Rename the type variables of one export `t0, t1, ...` in order of first
/// appearance, so a signature does not depend on the solver's counters.
fn renumber(params: &mut [osprey_types::Type], ret: &mut osprey_types::Type) -> usize {
    let mut names: HashMap<osprey_types::VarId, osprey_types::VarId> = HashMap::new();
    for ty in params.iter_mut().chain(std::iter::once(ret)) {
        rename_vars(ty, &mut names);
    }
    names.len()
}

fn rename_vars(
    ty: &mut osprey_types::Type,
    names: &mut HashMap<osprey_types::VarId, osprey_types::VarId>,
) {
    match ty {
        osprey_types::Type::Var(id) => {
            let next = u32::try_from(names.len()).unwrap_or(u32::MAX);
            *id = *names.entry(*id).or_insert(next);
        }
        osprey_types::Type::Con { args, .. } => {
            for arg in args.iter_mut() {
                rename_vars(arg, names);
            }
        }
        osprey_types::Type::Fun { params, ret } => {
            for param in params.iter_mut() {
                rename_vars(param, names);
            }
            rename_vars(ret, names);
        }
        osprey_types::Type::Record { fields, .. } => {
            for field in fields.iter_mut().map(|(_, field)| field) {
                rename_vars(field, names);
            }
        }
        osprey_types::Type::Union { .. } => {}
    }
}

/// `(namespace, module, exported function names)` for every module of the file.
fn exported_functions(source: &SourceFile) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for statement in &source.program.statements {
        if let Stmt::Namespace { name, body, .. } = statement {
            for inner in body {
                if let Stmt::Module { path, body, .. } = inner {
                    out.push((name.label().to_string(), path.to_string(), exports_of(body)));
                }
            }
        }
    }
    out
}

/// `kind name` for every exported item of module `module` in `source` that
/// is not a function: what the rendered signature leaves out.
fn omitted_of(source: &SourceFile, module: &str) -> Vec<String> {
    source
        .program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Namespace { body, .. } => Some(body),
            _ => None,
        })
        .flatten()
        .filter_map(|inner| match inner {
            Stmt::Module { path, body, .. } if path.to_string() == module => Some(body),
            _ => None,
        })
        .flatten()
        .filter(|item| item.visibility == osprey_ast::Visibility::Exported)
        .filter_map(|item| omitted_item(item.declaration.as_ref()))
        .collect()
}

fn omitted_item(declaration: &Stmt) -> Option<String> {
    match declaration {
        Stmt::Function { .. } => None,
        Stmt::Type { name, .. } => Some(format!("type {name}")),
        Stmt::Let { name, .. } => Some(format!("value {name}")),
        other => Some(format!("item {}", std::any::type_name_of_val(other))),
    }
}

fn exports_of(body: &[osprey_ast::ModuleItem]) -> Vec<String> {
    body.iter()
        .filter(|item| item.visibility == osprey_ast::Visibility::Exported)
        .filter_map(|item| match item.declaration.as_ref() {
            Stmt::Function { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn first_namespace(source: &SourceFile) -> String {
    source
        .program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Stmt::Namespace { name, .. } => Some(name.label().to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

/// The `signature Name` blocks of a checker's text, each with its name.
fn blocks(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        match line.strip_prefix("signature ") {
            Some(name) => out.push((name.trim().to_string(), format!("{line}\n"))),
            None => {
                if let Some((_, block)) = out.last_mut() {
                    block.push_str(line);
                    block.push('\n');
                }
            }
        }
    }
    out
}
