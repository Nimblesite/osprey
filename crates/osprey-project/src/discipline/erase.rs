//! Turning a checker's answer into the source the assembler sees
//! ([TYPES-DISCIPLINE-SEAM], [TYPES-DISCIPLINE-BOUNDARY]): the erased module
//! is parsed by the ordinary frontend of its flavor and every `module` in it
//! is ascribed to the signature the checker wrote, so `check_program` later
//! holds the erased code to that signature and importers see nothing else.

use super::protocol::{Diagnostic, Response};
use super::{failure, Discipline, ERROR_PREFIX};
use crate::{ProjectError, SourceFile};
use osprey_ast::{Program, SignatureAscription, Stmt, SymbolPath};
use osprey_syntax::Flavor;
use std::path::Path;

/// The erased source for `original` from an accepted response, or the
/// checker's diagnostics from a rejected one.
pub(super) fn erased_source(
    original: &SourceFile,
    discipline: &Discipline,
    response: Response,
) -> Result<SourceFile, ProjectError> {
    if !response.accepted() {
        return Err(rejected(original, discipline, &response.diagnostics));
    }
    let Some(erased) = response.erased else {
        return Err(failure(
            &original.path,
            discipline,
            "accepted the module but wrote no erased module",
        ));
    };
    let flavor = erased.flavor.parse::<Flavor>().map_err(|error| {
        failure(
            &original.path,
            discipline,
            &format!("named an unknown flavor: {error}"),
        )
    })?;
    let signatures = parse(
        &response.signature,
        Flavor::Ml,
        &original.path,
        discipline,
        "signature",
    )?;
    let mut program = parse(
        &erased.source,
        flavor,
        &original.path,
        discipline,
        "erased module",
    )?;
    ascribe(&mut program, &signatures, &original.path, discipline)?;
    Ok(SourceFile {
        path: original.path.clone(),
        flavor,
        source: erased.source,
        program,
        discipline: Some(discipline.name.clone()),
    })
}

/// The first diagnostic of a rejection, prefixed `<name>/`; the rest are
/// joined on following lines so nothing the checker said is lost.
fn rejected(
    original: &SourceFile,
    discipline: &Discipline,
    diagnostics: &[Diagnostic],
) -> ProjectError {
    let text = diagnostics
        .iter()
        .map(|d| format!("{}/{}: {}", discipline.name, d.code, d.message))
        .collect::<Vec<_>>()
        .join("\n");
    let first = diagnostics.first();
    ProjectError {
        message: if text.is_empty() {
            format!(
                "{ERROR_PREFIX}: checker `{}` rejected the module without a diagnostic",
                discipline.package
            )
        } else {
            text
        },
        path: Some(original.path.clone()),
        line: first.and_then(|d| d.line),
        column: first.and_then(|d| d.column),
    }
}

fn parse(
    text: &str,
    flavor: Flavor,
    path: &Path,
    discipline: &Discipline,
    what: &str,
) -> Result<Program, ProjectError> {
    let parsed = osprey_syntax::parse_program_with_flavor(text, flavor);
    match parsed.errors.first() {
        None => Ok(parsed.program),
        Some(error) => Err(failure(
            path,
            discipline,
            &format!(
                "wrote a {what} the {flavor} frontend cannot parse: {}:{}: {}",
                error.position.line, error.position.column, error.message
            ),
        )),
    }
}

/// Prepend the signature statements and ascribe every module to the signature
/// of its own name; a module the checker wrote no signature for is the
/// checker's fault ([TYPES-DISCIPLINE-SEAM]).
fn ascribe(
    program: &mut Program,
    signatures: &Program,
    path: &Path,
    discipline: &Discipline,
) -> Result<(), ProjectError> {
    let names: Vec<String> = signatures
        .statements
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Signature { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let missing = ascribe_all(&mut program.statements, &names);
    if let Some(module) = missing.first() {
        return Err(failure(
            path,
            discipline,
            &format!("wrote no signature for module `{module}`"),
        ));
    }
    let mut statements = signatures.statements.clone();
    statements.append(&mut program.statements);
    program.statements = statements;
    Ok(())
}

/// Ascribe modules at the top level and inside file-scoped namespaces,
/// returning the modules with no signature of their name.
fn ascribe_all(statements: &mut [Stmt], names: &[String]) -> Vec<String> {
    let mut missing = Vec::new();
    for statement in statements {
        match statement {
            Stmt::Module {
                path, signature, ..
            } => {
                let name = path.segments.join("::");
                if names.contains(&name) {
                    *signature = Some(SignatureAscription {
                        path: SymbolPath::new(path.segments.clone()),
                        allow_extra: false,
                    });
                } else {
                    missing.push(name);
                }
            }
            Stmt::Namespace { body, .. } => missing.append(&mut ascribe_all(body, names)),
            _ => {}
        }
    }
    missing
}
