//! Undocumented and unexampled public API. Implements [DOC-EXPORT-REQUIRED]
//! and [DOC-EXPORT-EXAMPLE-REQUIRED].
//!
//! An exported declaration is a library's contract with its callers, so it
//! must say what it is for. A module that exports anything must be documented
//! too, by an outer or an inner comment. Variants and fields are documented by
//! their type ([DOC-ATTACH]), so a documented type covers its constructors.
//!
//! A documented export a caller can call or read (a function, value, extern or
//! effect operation) must also show an example. Types, modules and signatures
//! are covered by the examples of the members that use them.

use osprey_ast::{DocComment, ModuleItem, Position, Program, Stmt, Visibility};

use crate::TypeWarning;

/// Shared rule identifier for compiler and editor diagnostics.
pub const UNDOCUMENTED_EXPORT: &str = "undocumented-export";

/// Shared rule identifier for an export whose documentation shows no example.
pub const UNEXAMPLED_EXPORT: &str = "unexampled-export";

/// Every undocumented or unexampled export in `program`, in source order.
#[must_use]
pub fn documentation_warnings(program: &Program) -> Vec<TypeWarning> {
    let mut warnings = Vec::new();
    statements(&program.statements, &mut warnings);
    warnings
}

fn statements(statements: &[Stmt], warnings: &mut Vec<TypeWarning>) {
    for statement in statements {
        match statement {
            Stmt::Namespace { body, .. } => self::statements(body, warnings),
            Stmt::Module { .. } => module(statement, "", false, warnings),
            _ => {}
        }
    }
}

/// A module, its exported items, and the modules nested in it. One exported
/// as an item is reported once, as that item, never again as a module.
fn module(statement: &Stmt, owner: &str, exported: bool, warnings: &mut Vec<TypeWarning>) {
    let Stmt::Module {
        path,
        body,
        doc,
        inner_doc,
        position,
        ..
    } = statement
    else {
        return;
    };
    let name = qualified(owner, &path.to_string());
    let exports = body
        .iter()
        .any(|item| item.visibility == Visibility::Exported);
    if exports && !exported && doc.is_none() && inner_doc.is_none() {
        warnings.push(warning(
            format!("module `{name}` exports an API but has no documentation comment"),
            *position,
            UNDOCUMENTED_EXPORT,
        ));
    }
    for item in body {
        self::item(item, &name, warnings);
    }
}

fn item(item: &ModuleItem, owner: &str, warnings: &mut Vec<TypeWarning>) {
    let exported = item.visibility == Visibility::Exported;
    let declaration = item.declaration.as_ref();
    if exported {
        if let Some((kind, name, position)) = undocumented(declaration) {
            let name = qualified(owner, &name);
            warnings.push(warning(
                format!("exported {kind} `{name}` has no documentation comment"),
                position,
                UNDOCUMENTED_EXPORT,
            ));
        }
        unexampled(declaration, owner, warnings);
    }
    module(declaration, owner, exported, warnings);
}

/// The kind, name and position of a declaration that carries no doc. A
/// module counts as documented by an outer or an inner comment.
fn undocumented(declaration: &Stmt) -> Option<(String, String, Option<Position>)> {
    if let Stmt::Module {
        path,
        doc,
        inner_doc,
        position,
        ..
    } = declaration
    {
        let documented = doc.is_some() || inner_doc.is_some();
        return (!documented).then(|| ("module".to_owned(), path.to_string(), *position));
    }
    let declared = declaration.documented()?;
    let kind = declared.kind.to_lowercase();
    declared
        .doc
        .is_none()
        .then(|| (kind, declared.name.to_owned(), declared.position))
}

/// Reports a documented callable export, or an operation of an exported
/// effect, whose documentation holds no example fence. An undocumented
/// function, value or extern is already reported as undocumented.
fn unexampled(declaration: &Stmt, owner: &str, warnings: &mut Vec<TypeWarning>) {
    if let Stmt::Effect {
        name, operations, ..
    } = declaration
    {
        let effect = qualified(owner, name);
        for operation in operations
            .iter()
            .filter(|op| !shows_example(op.doc.as_ref()))
        {
            warnings.push(no_example(
                "operation",
                &format!("{effect}.{}", operation.name),
                operation.position,
            ));
        }
        return;
    }
    let Some(declared) = declaration.documented() else {
        return;
    };
    let callable = matches!(declared.kind, "Function" | "Value" | "Extern");
    if callable && declared.doc.is_some() && !shows_example(declared.doc) {
        let kind = declared.kind.to_lowercase();
        warnings.push(no_example(
            &kind,
            &qualified(owner, declared.name),
            declared.position,
        ));
    }
}

fn shows_example(doc: Option<&DocComment>) -> bool {
    doc.is_some_and(|doc| !doc.examples.is_empty())
}

fn no_example(kind: &str, name: &str, position: Option<Position>) -> TypeWarning {
    warning(
        format!("exported {kind} `{name}` has no example"),
        position,
        UNEXAMPLED_EXPORT,
    )
}

fn qualified(owner: &str, name: &str) -> String {
    if owner.is_empty() {
        name.to_owned()
    } else {
        format!("{owner}::{name}")
    }
}

fn warning(message: String, position: Option<Position>, rule: &'static str) -> TypeWarning {
    TypeWarning {
        message,
        position,
        rule,
    }
}
