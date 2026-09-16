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
use osprey_ast::{EffectRef, Expr, ExternParameter, Parameter, Position, Stmt, TypeExpr};
use osprey_lsp::analysis::render_type;
use osprey_syntax::Flavor;

/// The derived sections for one page, in reading order.
///
/// A section is dropped when the declaration says nothing about it, and when
/// the author's own comment already covers it: `render_markdown` has emitted
/// theirs, and one page may not carry two `## Parameters`.
pub(super) fn sections(entry: &DocEntry, flavor: Flavor, location: &str) -> String {
    let (taken, result) = shape(entry, flavor);
    [
        parameters(entry, &taken, flavor),
        returns(entry, result),
        effects(entry, flavor),
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
/// of the signature, so it belongs on the signature.
pub(super) fn effect_row(entry: &DocEntry, flavor: Flavor) -> String {
    let Some(Stmt::Function { effects, .. }) = entry.declaration.as_ref() else {
        return String::new();
    };
    let row: Vec<String> = effects
        .iter()
        .map(|effect| format!("{}{}", effect.name, arguments(effect, flavor)))
        .collect();
    match (flavor, row.as_slice()) {
        (_, []) => String::new(),
        (Flavor::Ml, [only]) => format!(" ! {only}"),
        (Flavor::Ml, many) => format!(" ! [{}]", many.join(", ")),
        (Flavor::Default, many) => format!(" ![{}]", many.join(", ")),
    }
}

/// The parameters a caller passes and the result they get back.
fn shape(entry: &DocEntry, flavor: Flavor) -> (Vec<String>, Option<String>) {
    match entry.declaration.as_ref() {
        Some(Stmt::Function {
            parameters,
            return_type,
            body,
            ..
        }) => uncurried(parameters, return_type.as_ref(), body, flavor),
        Some(Stmt::Extern {
            parameters,
            return_type,
            ..
        }) => (
            parameters.iter().map(|p| external(p, flavor)).collect(),
            return_type.as_ref().map(|ty| spelled(ty, flavor)),
        ),
        _ => (Vec::new(), None),
    }
}

/// Undo the currying ML lowering applied, so the page lists the arguments a
/// call site actually writes.
///
/// `route method path body` lowers to a ONE-parameter function whose body is a
/// chain of one-parameter lambdas ([FLAVOR-ML-CURRY]); the declaration node
/// alone names `method` and stops. A page built from that tells a reader a
/// three-argument function takes one argument, which is worse than silence.
/// Default functions carry every parameter on the declaration, so a lambda in
/// one of those bodies is a value the author returns and the walk never starts.
fn uncurried(
    parameters: &[Parameter],
    return_type: Option<&TypeExpr>,
    body: &Expr,
    flavor: Flavor,
) -> (Vec<String>, Option<String>) {
    let (taken, result) = walk(parameters, return_type, body, flavor);
    (
        taken.iter().map(|p| declared(p, flavor)).collect(),
        result.map(|ty| spelled(ty, flavor)),
    )
}

/// Follow the lambda chain to its end, gathering what each link takes and
/// keeping the result type of the last one reached.
fn walk<'a>(
    parameters: &'a [Parameter],
    return_type: Option<&'a TypeExpr>,
    body: &'a Expr,
    flavor: Flavor,
) -> (Vec<&'a Parameter>, Option<&'a TypeExpr>) {
    let mut taken = passed(parameters);
    let mut result = return_type;
    let mut rest = (flavor == Flavor::Ml).then_some(body);
    while let Some(Expr::Lambda {
        parameters,
        return_type,
        body,
        ..
    }) = rest
    {
        taken.extend(passed(parameters));
        result = return_type.as_ref();
        rest = Some(body.as_ref());
    }
    (taken, result)
}

/// A lowered self-alias is part of the source parameter it constrains, not one
/// a caller passes.
fn passed(parameters: &[Parameter]) -> Vec<&Parameter> {
    parameters
        .iter()
        .filter(|parameter| !parameter.inline_constraint)
        .collect()
}

fn declared(parameter: &Parameter, flavor: Flavor) -> String {
    takes(
        &parameter.name,
        parameter.ty.as_ref().map(|ty| spelled(ty, flavor)),
    )
}

fn external(parameter: &ExternParameter, flavor: Flavor) -> String {
    takes(&parameter.name, Some(spelled(&parameter.ty, flavor)))
}

/// One parameter with the type it was written with, or on its own where the
/// type is inferred — the name is still what a caller needs.
fn takes(name: &str, ty: Option<String>) -> String {
    ty.map_or_else(
        || format!("- `{name}`"),
        |ty| format!("- `{name}` — `{ty}`"),
    )
}

/// One written type in the flavor its page is authored in.
///
/// The presentation edge respells a whole signature line, so a bare type is
/// offered to it as one and taken back off again: an ML page must never show
/// `fn(int) -> int`, a spelling its author could not have written
/// ([FLAVOR-BOUNDARY]). A line it declines to respell is returned unchanged,
/// so the type survives either way.
fn spelled(ty: &TypeExpr, flavor: Flavor) -> String {
    let rendered = render_type(ty);
    let line = osprey_lsp::source_signature(flavor, &format!("t: {rendered}"));
    line.split_once(':')
        .map_or(rendered, |(_, ty)| ty.trim().to_owned())
}

/// `## Parameters` read off the declaration, where that adds anything.
///
/// An ML signature line carries types and no names ([FLAVOR-ML-FN]):
/// `string -> string -> R` says nothing about which string is which, and the
/// names are information the page does not otherwise hold. A Default signature
/// line already names every parameter, so the same list under it would restate
/// the line above with less in it — worse than no section at all.
fn parameters(entry: &DocEntry, taken: &[String], flavor: Flavor) -> String {
    if !entry.doc.params.is_empty() || taken.is_empty() || flavor == Flavor::Default {
        return String::new();
    }
    format!("## Parameters\n\n{}", taken.join("\n"))
}

/// `## Returns`, for a declaration whose result type was written down.
fn returns(entry: &DocEntry, result: Option<String>) -> String {
    if entry.doc.returns.is_some() {
        return String::new();
    }
    result.map_or_else(String::new, |ty| format!("## Returns\n\n`{ty}`"))
}

/// `## Effects` — the row the declaration was written with.
///
/// This is the fact a caller most needs and the one the type alone never
/// carries: the compiler rejects a program that performs an effect no handler
/// discharges, so the row has to be known before the call is written.
fn effects(entry: &DocEntry, flavor: Flavor) -> String {
    let Some(Stmt::Function { effects, .. }) = entry.declaration.as_ref() else {
        return String::new();
    };
    let rows: Vec<String> = effects
        .iter()
        .map(|effect| performed(effect, flavor))
        .collect();
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
fn performed(effect: &EffectRef, flavor: Flavor) -> String {
    match arguments(effect, flavor) {
        args if args.is_empty() => format!("- [{}]", effect.name),
        args => format!("- [{}]`{args}`", effect.name),
    }
}

/// An effect's type arguments, spelled as both flavors spell them: angle
/// brackets on a row reference, in the flavor's own rendering of each argument.
fn arguments(effect: &EffectRef, flavor: Flavor) -> String {
    if effect.type_args.is_empty() {
        return String::new();
    }
    let args: Vec<String> = effect
        .type_args
        .iter()
        .map(|ty| spelled(ty, flavor))
        .collect();
    format!("<{}>", args.join(", "))
}

/// Where to open the declaration in the reader's own checkout.
///
/// A closing line rather than a section of its own: a namespace contributed to
/// from several files merges into one page, and two identical `##` headings on
/// one page is a worse outcome than a plain sentence.
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
