//! Imports nothing resolves through. Implements [MODULES-IMPORT-UNUSED].
//!
//! An import is a dependency the reader must account for; one no name uses
//! is a dependency that is not there. Resolution records each import binding
//! a name resolved through (a qualified path, a bare member, a type, a
//! method-style call), so an import none of whose bindings was used is
//! reported, spelled as written. A module of the importing namespace resolves
//! before any import, so importing it is also reported, except from a
//! Python-flavor file, which names a sibling module only through an import.
//! A project with a discipline is not reported at all: the discipline's
//! checker reads every file's imports directly. A wildcard import
//! binds names nobody wrote, so it is never reported.

use std::collections::BTreeSet;

use osprey_ast::{ImportDecl, ImportMember, ImportSelection};

use crate::contribution::Contribution;
use crate::model::SymbolKey;
use crate::{ProjectWarning, SourceMetadata};

use osprey_types::UNUSED_IMPORT;

/// One warning per import of `contributions` whose bindings `used` never names.
pub(crate) fn warnings(
    contributions: &[Contribution],
    used: &BTreeSet<(usize, String)>,
    referenced: &BTreeSet<(usize, SymbolKey)>,
    sources: &[SourceMetadata],
) -> Vec<ProjectWarning> {
    // A discipline's checker reads imports itself: each importing file's
    // checker receives the imported modules' export signatures
    // ([TYPES-DISCIPLINE-REGISTRY]), so in a project with a discipline every
    // import is used whether or not a name resolves through it.
    if sources.iter().any(|source| source.discipline.is_some()) {
        return Vec::new();
    }
    contributions
        .iter()
        .enumerate()
        .flat_map(|(index, contribution)| {
            contribution
                .imports
                .iter()
                .filter(move |import| unused(index, import, used))
                .filter(move |import| !python_sibling(contribution, import, sources))
                .map(move |import| ProjectWarning {
                    rule: UNUSED_IMPORT,
                    message: message(index, import, referenced),
                    position: crate::advice::position(
                        import.position,
                        contribution.source,
                        sources,
                    ),
                })
        })
        .collect()
}

/// A Python-flavor file names its sibling modules only through an import, as
/// Python does, so its import of a module of its own namespace is required by
/// the flavor, not unused.
fn python_sibling(
    contribution: &Contribution,
    import: &ImportDecl,
    sources: &[SourceMetadata],
) -> bool {
    let python = sources
        .get(contribution.source)
        .is_some_and(|source| source.flavor == osprey_syntax::Flavor::Python);
    python && import.target.namespace.label() == contribution.namespace.label()
}

fn unused(index: usize, import: &ImportDecl, used: &BTreeSet<(usize, String)>) -> bool {
    let bindings = bindings(import);
    !bindings.is_empty()
        && bindings
            .into_iter()
            .all(|binding| !used.contains(&(index, binding)))
}

/// "never used" when no name of the file refers to the imported target at
/// all; "redundant" when names do, through the file's own namespace.
fn message(index: usize, import: &ImportDecl, referenced: &BTreeSet<(usize, SymbolKey)>) -> String {
    let target = target(import);
    let named = referenced.iter().any(|(owner, key)| {
        *owner == index && key.namespace == target.namespace && key.path.starts_with(&target.path)
    });
    if named {
        let module = target.source_name();
        format!("import `{}` is redundant: `{module}` is in this file's namespace and resolves without it", spelled(import))
    } else {
        format!("import `{}` is never used", spelled(import))
    }
}

fn target(import: &ImportDecl) -> SymbolKey {
    SymbolKey::new(
        import.target.namespace.label(),
        import.target.path.segments.clone(),
    )
}

/// The local names an import introduces.
fn bindings(import: &ImportDecl) -> Vec<String> {
    match &import.selection {
        ImportSelection::Whole => crate::imports::whole_alias(import).into_iter().collect(),
        ImportSelection::Members(members) => members
            .iter()
            .map(|member| member.alias.clone().unwrap_or_else(|| member.name.clone()))
            .collect(),
        ImportSelection::Wildcard => Vec::new(),
    }
}

/// The import as Default writes it: `a::B`, `a::B as C` or `a::B::{x, y as z}`.
fn spelled(import: &ImportDecl) -> String {
    let target = target(import).source_name();
    match &import.selection {
        ImportSelection::Whole => match &import.alias {
            Some(alias) => format!("{target} as {alias}"),
            None => target,
        },
        ImportSelection::Members(members) => {
            let listed: Vec<String> = members.iter().map(member).collect();
            format!("{target}::{{{}}}", listed.join(", "))
        }
        ImportSelection::Wildcard => format!("{target}::*"),
    }
}

fn member(member: &ImportMember) -> String {
    match &member.alias {
        Some(alias) => format!("{} as {alias}", member.name),
        None => member.name.clone(),
    }
}
