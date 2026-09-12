//! What a declaration states about itself, for the public members nobody wrote
//! a comment for. Implements [DOC-EXPORT].
//!
//! Every line here is read off the declaration the compiler already accepted:
//! parameter names, the types that were written down, the effect row, the file
//! and line. Nothing is inferred prose. A public function without a `///` still
//! has to tell a reader what it takes, what it answers with and what the caller
//! has to handle — a page carrying only a name and a bare type does not, and a
//! reader who opens one learns nothing they did not already know.

use crate::document_entries::DocEntry;
use osprey_ast::{EffectRef, ExternParameter, Parameter, Position, Stmt};
use osprey_lsp::analysis::render_type;
use osprey_syntax::Flavor;

/// The derived sections for one page, in reading order.
///
/// A section is dropped when the declaration says nothing about it, and when
/// the author's own comment already covers it: `render_markdown` has emitted
/// theirs, and one page may not carry two `## Parameters`.
pub(super) fn sections(entry: &DocEntry, location: &str) -> String {
    [
        parameters(entry),
        returns(entry),
        effects(entry),
        defined_in(entry, location),
    ]
    .into_iter()
    .filter(|section| !section.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
}

/// The declared effect row in `flavor`'s own spelling, empty where none was
/// written.
///
/// The editor's type model renders a function's *value* type and stops there,
/// so a signature line built from it alone reads as pure for a function the
/// compiler will reject unless its caller discharges the row. The row is part
/// of the signature, so it is shown on the signature.
pub(super) fn effect_row(entry: &DocEntry, flavor: Flavor) -> String {
    let Some(Stmt::Function { effects, .. }) = entry.declaration.as_ref() else {
        return String::new();
    };
    match (flavor, performed_by(effects).as_slice()) {
        (_, []) => String::new(),
        (Flavor::Ml, [only]) => format!(" ! {only}"),
        (Flavor::Ml, row) => format!(" ! [{}]", row.join(", ")),
        (Flavor::Default, row) => format!(" ![{}]", row.join(", ")),
    }
}

fn performed_by(effects: &[EffectRef]) -> Vec<String> {
    effects
        .iter()
        .map(|effect| format!("{}{}", effect.name, arguments(effect)))
        .collect()
}

/// `## Parameters` read off the declaration.
///
/// An ML signature line carries types and no names (`string -> string -> R`
/// says nothing about which string is which), so the names are information the
/// page does not otherwise hold.
fn parameters(entry: &DocEntry) -> String {
    if !entry.doc.params.is_empty() {
        return String::new();
    }
    let rows = match entry.declaration.as_ref() {
        Some(Stmt::Function { parameters, .. }) => declared(parameters),
        Some(Stmt::Extern { parameters, .. }) => external(parameters),
        _ => Vec::new(),
    };
    if rows.is_empty() {
        return String::new();
    }
    format!("## Parameters\n\n{}", rows.join("\n"))
}

/// A lowered self-alias is part of the source parameter it constrains, not a
/// parameter the author wrote, so it is not one a reader passes.
fn declared(parameters: &[Parameter]) -> Vec<String> {
    parameters
        .iter()
        .filter(|parameter| !parameter.inline_constraint)
        .map(|parameter| takes(&parameter.name, parameter.ty.as_ref().map(render_type)))
        .collect()
}

fn external(parameters: &[ExternParameter]) -> Vec<String> {
    parameters
        .iter()
        .map(|parameter| takes(&parameter.name, Some(render_type(&parameter.ty))))
        .collect()
}

/// One parameter with the type it was written with, or on its own where the
/// type is inferred — the name is still what a caller needs.
fn takes(name: &str, ty: Option<String>) -> String {
    ty.map_or_else(
        || format!("- `{name}`"),
        |ty| format!("- `{name}` — `{ty}`"),
    )
}

/// `## Returns`, for a declaration whose result type was written down.
fn returns(entry: &DocEntry) -> String {
    if entry.doc.returns.is_some() {
        return String::new();
    }
    let declared = match entry.declaration.as_ref() {
        Some(Stmt::Function { return_type, .. } | Stmt::Extern { return_type, .. }) => {
            return_type.as_ref()
        }
        _ => None,
    };
    declared.map_or_else(String::new, |ty| {
        format!("## Returns\n\n`{}`", render_type(ty))
    })
}

/// `## Effects` — the row the declaration was written with.
///
/// This is the fact a caller most needs and the one the type alone never
/// carries: the compiler rejects a program that performs an effect no handler
/// discharges, so the row has to be known before the call is written.
fn effects(entry: &DocEntry) -> String {
    let Some(Stmt::Function { effects, .. }) = entry.declaration.as_ref() else {
        return String::new();
    };
    let rows: Vec<String> = effects.iter().map(operation).collect();
    if rows.is_empty() {
        return String::new();
    }
    format!(
        "## Effects\n\nCalling this asks for the work below. A caller runs it inside a matching \
         `handle`, which decides how that work is actually done.\n\n{}",
        rows.join("\n")
    )
}

/// One effect as a documentation symbol link, so it reaches the effect's own
/// page wherever this export documents it ([DOC-LINK]).
fn operation(effect: &EffectRef) -> String {
    match arguments(effect) {
        args if args.is_empty() => format!("- [{}]", effect.name),
        args => format!("- [{}]`{args}`", effect.name),
    }
}

fn arguments(effect: &EffectRef) -> String {
    if effect.type_args.is_empty() {
        return String::new();
    }
    let args: Vec<String> = effect.type_args.iter().map(render_type).collect();
    format!("<{}>", args.join(", "))
}

/// Where to open the declaration in the reader's own checkout.
///
/// A closing line rather than a section: a namespace contributed to from
/// several files merges into one page, and two identical `##` headings on one
/// page is a worse outcome than a plain sentence.
fn defined_in(entry: &DocEntry, location: &str) -> String {
    position(entry).map_or_else(String::new, |position| {
        format!("*Defined in `{location}`, line {}.*", position.line)
    })
}

fn position(entry: &DocEntry) -> Option<Position> {
    match entry.declaration.as_ref()? {
        Stmt::Function { position, .. }
        | Stmt::Let { position, .. }
        | Stmt::Extern { position, .. }
        | Stmt::Type { position, .. }
        | Stmt::Effect { position, .. }
        | Stmt::Signature { position, .. }
        | Stmt::Module { position, .. } => *position,
        _ => None,
    }
}
