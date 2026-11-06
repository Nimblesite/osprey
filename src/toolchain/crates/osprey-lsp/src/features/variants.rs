//! A union's variants as things an editor can navigate to: `Clock => "clock"`
//! hovers and defines like any other name, though a variant is no symbol of its
//! own. Implements [LSP-DEFINITION-VARIANTS] and [LSP-HOVER-VARIANTS].

use lspkit::vfs::PositionEncoding;
use osprey_ast::{is_positional_field, DocComment, Position, Stmt, TypeVariant};
use osprey_syntax::Flavor;

use crate::mlrender;
use crate::model::Span;
use crate::text::occurrences;

/// A variant and the union that declares it.
struct Declared<'a> {
    owner: &'a str,
    variant: &'a TypeVariant,
    doc: Option<&'a DocComment>,
    position: Option<Position>,
}

/// The union variant `name` in `stmts`, looking inside namespaces and modules. A
/// record's one variant is the type itself, which is already a symbol.
fn declared<'a>(stmts: &mut dyn Iterator<Item = &'a Stmt>, name: &str) -> Option<Declared<'a>> {
    for stmt in stmts {
        let found = match stmt {
            Stmt::Namespace { body, .. } => declared(&mut body.iter(), name),
            Stmt::Module { body, .. } => {
                declared(&mut body.iter().map(|item| item.declaration.as_ref()), name)
            }
            Stmt::Type {
                name: owner,
                variants,
                doc,
                position,
                ..
            } => variants
                .iter()
                .find(|variant| variant.name == name && owner != name)
                .map(|variant| Declared {
                    owner,
                    variant,
                    doc: doc.as_ref(),
                    position: *position,
                }),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// What a variant carries, as its union's declaration writes it.
fn written_fields(variant: &TypeVariant) -> String {
    let positional = variant.fields.iter().all(|f| is_positional_field(&f.name));
    let rendered = variant.fields.iter().map(|field| {
        if positional {
            field.ty.clone()
        } else {
            format!("{}: {}", field.name, field.ty)
        }
    });
    let rendered = rendered.collect::<Vec<_>>().join(", ");
    match (variant.fields.is_empty(), positional) {
        (true, _) => String::new(),
        (false, true) => format!("({rendered})"),
        (false, false) => format!(" {{ {rendered} }}"),
    }
}

/// The hover for `word` when it names a variant in `stmts`: the variant, what it
/// carries, its union, and the union's documentation.
pub(crate) fn hover(stmts: &[Stmt], word: &str, flavor: Flavor) -> Option<String> {
    let found = declared(&mut stmts.iter(), leaf(word))?;
    let code = format!(
        "{}{}: {}",
        found.variant.name,
        written_fields(found.variant),
        found.owner
    );
    let mut parts = vec![format!("```{}\n{code}\n```", mlrender::fence(flavor))];
    parts.extend(found.doc.map(DocComment::render_markdown));
    Some(parts.join("\n\n"))
}

/// Where `word` is declared as a variant in `text`, whose program is `stmts`: its
/// first occurrence from the union's declaration on.
pub(crate) fn definition(
    stmts: &[Stmt],
    text: &str,
    word: &str,
    enc: PositionEncoding,
) -> Option<Span> {
    let name = leaf(word);
    let from = declared(&mut stmts.iter(), name)?
        .position
        .map_or(0, |position| position.line.saturating_sub(1));
    occurrences(text, name, enc)
        .into_iter()
        .find(|occurrence| occurrence.line >= from)
        .map(|found| (found.line, found.start, found.line, found.end))
}

fn leaf(word: &str) -> &str {
    word.rsplit("::").next().unwrap_or(word)
}
