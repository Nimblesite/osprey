//! ES module syntax over Osprey namespaces, modules and the program entry.
//! Implements [FLAVOR-TS-INTEROP], [FLAVOR-TS-ENTRY], [FLAVOR-TS-SELECT].
use super::context::{Lowerer, Result};
use super::statements::receiver_roots;
use crate::file_header::{identifier, Directives};
use osprey_ast::{
    ImportDecl, ImportMember, ImportSelection, ImportTarget, NamespaceName, Program, Stmt,
    SymbolPath,
};
use oxc_ast::ast::{
    Declaration, ImportDeclaration, ImportDeclarationSpecifier, ModuleExportName, Statement,
};
use oxc_span::{GetSpan, Span};

/// The program entry [MODULES-ENTRYPOINT] top-level statements form.
const ENTRY: &str = "main";

/// `router-utils` names module `RouterUtils`.
fn module_name(stem: &str) -> String {
    super::types::variant_name(stem)
}

impl Lowerer<'_> {
    pub(super) fn program(&mut self, program: &oxc_ast::ast::Program<'_>) -> Program {
        let header = self.header(program);
        let mut updated = Vec::new();
        receiver_roots(&program.body, &mut updated);
        let mut file = Vec::new();
        let mut entry = Vec::new();
        for statement in &program.body {
            let started = !entry.is_empty();
            match self.top_level(statement, started, &updated) {
                Ok(Some(mut declarations)) => file.append(&mut declarations),
                Ok(None) => entry.push(statement),
                Err(error) => self.errors.push(error),
            }
        }
        if !entry.is_empty() {
            match self.entry(&entry, program.span) {
                Ok(main) => file.push(main),
                Err(error) => self.errors.push(error),
            }
        }
        let exports = std::mem::take(&mut self.exports);
        let file = header.module_contribution(file, |name| exports.contains(name), None);
        Program {
            statements: header.namespace_contribution(file),
            doc: None,
        }
    }

    /// The directives in the comments before the first statement.
    fn header(&mut self, program: &oxc_ast::ast::Program<'_>) -> Directives {
        let first_code = program
            .body
            .first()
            .map_or(u32::MAX, |first| first.span().start);
        let mut header = Directives::default();
        for comment in &program.comments {
            let text = self.text(comment.span).trim_start_matches('/').trim();
            let Some(directive) = text.strip_prefix("osprey:") else {
                continue;
            };
            let position = self.at(comment.span);
            let read = if comment.span.start < first_code {
                header.directive(directive.trim(), position)
            } else {
                Err(self.error(
                    comment.span,
                    "Osprey directives must precede source declarations",
                ))
            };
            self.errors.extend(read.err());
        }
        self.errors
            .extend(header.check(self.at(program.span)).err());
        self.namespace = header.namespace.as_ref().map(|(name, _)| name.clone());
        header
    }

    /// A file-scope declaration, or `None` for a statement of the entry.
    fn top_level(
        &mut self,
        statement: &Statement<'_>,
        entry_started: bool,
        updated: &[&str],
    ) -> Result<Option<Vec<Stmt>>> {
        let declarations = match statement {
            Statement::ImportDeclaration(import) => self.import(import)?,
            Statement::ExportDeclaration(export) => self.exported(&export.declaration)?,
            Statement::FunctionDeclaration(function) => vec![self.function(function)?],
            Statement::TSInterfaceDeclaration(interface) => vec![self.interface(interface)?],
            Statement::TSTypeAliasDeclaration(alias) => vec![self.type_alias(alias)?],
            Statement::EmptyStatement(_) => Vec::new(),
            Statement::VariableDeclaration(declaration)
                if !entry_started && !self.declares_any(declaration, updated) =>
            {
                self.variables(declaration)?
            }
            Statement::ExportDefaultDeclaration(_)
            | Statement::ExportAllDeclaration(_)
            | Statement::ExportNamedDeclaration(_)
            | Statement::ExportFromDeclaration(_) => {
                return self.unsupported(
                    statement.span(),
                    "`export default`, export list or re-export; write `export` on the declaration",
                );
            }
            _ => return Ok(None),
        };
        Ok(Some(declarations))
    }

    fn declares_any(
        &self,
        declaration: &oxc_ast::ast::VariableDeclaration<'_>,
        names: &[&str],
    ) -> bool {
        declaration.declarations.iter().any(|declarator| {
            self.binding_name(&declarator.id)
                .is_ok_and(|name| names.contains(&name.as_str()))
        })
    }

    fn exported(&mut self, declaration: &Declaration<'_>) -> Result<Vec<Stmt>> {
        let declarations = match declaration {
            Declaration::FunctionDeclaration(function) => vec![self.function(function)?],
            Declaration::TSInterfaceDeclaration(interface) => vec![self.interface(interface)?],
            Declaration::TSTypeAliasDeclaration(alias) => vec![self.type_alias(alias)?],
            Declaration::VariableDeclaration(variables) => self.variables(variables)?,
            other => return self.unsupported(other.span(), "exported declaration form"),
        };
        for name in declarations
            .iter()
            .filter_map(osprey_ast::stmt_meta::declaration_name)
        {
            let _ = self.exports.insert(name.to_owned());
        }
        Ok(declarations)
    }

    /// Top-level statements are the program entry, in source order.
    fn entry(&mut self, statements: &[&Statement<'_>], span: Span) -> Result<Stmt> {
        let body = self.block(statements, span)?;
        Ok(Stmt::Function {
            name: ENTRY.to_owned(),
            type_params: Vec::new(),
            parameters: Vec::new(),
            return_type: None,
            effects: Vec::new(),
            effect_tail: None,
            effect_row_present: false,
            body,
            doc: None,
            position: Some(self.at(span)),
        })
    }

    fn import(&mut self, import: &ImportDeclaration<'_>) -> Result<Vec<Stmt>> {
        let Some(specifiers) = &import.specifiers else {
            return self.unsupported(import.span, "side-effect import");
        };
        let source = self.string(&import.source)?;
        let (target, package) = self.import_target(&source, import.source.span)?;
        let mut members = Vec::new();
        let mut imports = Vec::new();
        for specifier in specifiers {
            match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    members.push(self.import_member(&named.imported, named.local.name.as_str())?);
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                    let Some(package) = &package else {
                        return self.unsupported(default.span, "default import of a project file");
                    };
                    let alias = default.local.name.as_str().to_owned();
                    let _ = self.modules.insert(alias.clone(), Some(package.clone()));
                    imports.push(self.import_decl(
                        &target,
                        Some(alias),
                        ImportSelection::Whole,
                        import.span,
                    ));
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(all) => {
                    return self.unsupported(all.span, "`import *`");
                }
            }
        }
        if !members.is_empty() {
            let selection = ImportSelection::Members(members);
            imports.push(self.import_decl(&target, None, selection, import.span));
        }
        Ok(imports)
    }

    fn import_decl(
        &self,
        target: &ImportTarget,
        alias: Option<String>,
        selection: ImportSelection,
        span: Span,
    ) -> Stmt {
        Stmt::Import(ImportDecl {
            target: target.clone(),
            alias,
            selection,
            position: Some(self.at(span)),
        })
    }

    fn import_member(&self, imported: &ModuleExportName<'_>, local: &str) -> Result<ImportMember> {
        let ModuleExportName::IdentifierName(name) = imported else {
            return self.unsupported(imported.span(), "string-named import");
        };
        let name = name.name.as_str();
        Ok(ImportMember {
            name: name.to_owned(),
            alias: (name != local).then(|| local.to_owned()),
        })
    }

    /// `"./router"` is module `Router` of this file's namespace; `"express"` is
    /// module `Express` of namespace `express`, whose entry function a default
    /// import calls. Returns the target and, for a package, its name.
    fn import_target(&self, source: &str, span: Span) -> Result<(ImportTarget, Option<String>)> {
        let relative = source.starts_with("./") || source.starts_with("../");
        let stem = source.rsplit('/').next().unwrap_or(source);
        let stem = stem.strip_suffix(".ts").unwrap_or(stem);
        let namespace = if relative {
            self.namespace.clone().ok_or_else(|| {
                self.error(
                    span,
                    "a relative import needs a `// osprey: namespace=` directive",
                )
            })?
        } else {
            stem.to_owned()
        };
        if !identifier(&namespace) || !identifier(&module_name(stem)) {
            return Err(self.error(
                span,
                "an import source must name an Osprey namespace or module",
            ));
        }
        let target = ImportTarget {
            namespace: NamespaceName::Identifier(namespace.clone()),
            path: SymbolPath::single(module_name(stem)),
        };
        Ok((target, (!relative).then_some(namespace)))
    }
}
