//! Effect-operation hover and implementation navigation.
//!
//! Operations are identified by both their owning effect and operation name.
//! That keeps `Trace.mark` distinct from `Other.mark`, while one AST-derived
//! index serves declaration, `perform`, handler-arm hover, and implementation
//! queries. An effect is identified by its declaration, so every
//! instantiation `Tensor<List<float>>` discharges `Tensor`. Implements
//! [LSP-HOVER-EFFECT-OPERATIONS] and [LSP-IMPLEMENTATIONS-EFFECT-HANDLERS].

use lspkit::vfs::PositionEncoding;
use osprey_ast::effect_name::base;
use osprey_ast::{
    walk_program, AstVisitor, DocComment, EffectOperation, Expr, Position, Program, SignatureItem,
    Stmt,
};
use osprey_syntax::Flavor;

use crate::analysis::{collect_symbols, SymbolInfo};
use crate::features::nth_line;
use crate::text::{measure, prefix_to, word_at, WordSpan};
use crate::{mlrender, workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperationId {
    effect: String,
    operation: String,
}

#[derive(Debug, Clone)]
struct Declaration {
    id: OperationId,
    ty: String,
    doc: Option<String>,
    position: Option<Position>,
}

#[derive(Debug, Clone)]
struct Site {
    id: OperationId,
    position: Option<Position>,
}

/// A handler region: the effect it discharges, at its `handle` keyword.
#[derive(Debug, Clone)]
struct Discharge {
    effect: String,
    position: Option<Position>,
}

#[derive(Debug, Default)]
struct EffectIndex {
    declarations: Vec<Declaration>,
    /// Operations a `signature` promises. They resolve a query made on the
    /// contract, while hover keeps reading the module's own declaration.
    contracts: Vec<Declaration>,
    performs: Vec<Site>,
    handlers: Vec<Site>,
    discharges: Vec<Discharge>,
    effect_symbols: Vec<SymbolInfo>,
}

/// Hover markdown for an operation declaration, `perform`, or handler arm.
pub(crate) fn operation_hover(
    program: &Program,
    source: &str,
    siblings: &[workspace::Sibling],
    line: u32,
    character: u32,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> Option<String> {
    let index = effect_index(program);
    let id = target_at(&index, source, line, character, encoding, flavor)?;
    let declaration = declaration(&index, siblings, &id)?;
    Some(render_hover(&declaration, flavor))
}

fn declaration(
    index: &EffectIndex,
    siblings: &[workspace::Sibling],
    id: &OperationId,
) -> Option<Declaration> {
    index
        .declarations
        .iter()
        .find(|declaration| same_operation(&declaration.id, id))
        .cloned()
        .or_else(|| {
            siblings.iter().find_map(|sibling| {
                effect_index(&sibling.program)
                    .declarations
                    .into_iter()
                    .find(|declaration| same_operation(&declaration.id, id))
            })
        })
}

fn same_operation(left: &OperationId, right: &OperationId) -> bool {
    left.operation == right.operation && same_effect(&left.effect, &right.effect)
}

fn same_effect(left: &str, right: &str) -> bool {
    left == right || qualified_suffix(left, right) || qualified_suffix(right, left)
}

fn qualified_suffix(qualified: &str, suffix: &str) -> bool {
    qualified
        .strip_suffix(suffix)
        .is_some_and(|prefix| prefix.ends_with("::"))
}

fn render_hover(declaration: &Declaration, flavor: Flavor) -> String {
    let code = format!(
        "{}.{}: {}",
        declaration.id.effect, declaration.id.operation, declaration.ty
    );
    let mut markdown = mlrender::fenced(flavor, &code);
    if let Some(doc) = declaration.doc.as_deref().filter(|doc| !doc.is_empty()) {
        markdown.push_str("\n\n");
        markdown.push_str(doc);
    }
    markdown
}

fn target_at(
    index: &EffectIndex,
    source: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> Option<OperationId> {
    let text = nth_line(source, line)?;
    let span = word_at(text, character, encoding)?;
    index
        .declarations
        .iter()
        .chain(&index.contracts)
        .find(|declaration| {
            positioned_at(declaration.position, text, line, &span, encoding, flavor)
        })
        .map(|declaration| declaration.id.clone())
        .or_else(|| positioned_handler(index, text, line, &span, encoding, flavor))
        .or_else(|| performed_operation(index, text, line, &span, encoding))
}

fn positioned_handler(
    index: &EffectIndex,
    text: &str,
    line: u32,
    span: &WordSpan,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> Option<OperationId> {
    index
        .handlers
        .iter()
        .find(|site| {
            site.id.operation == span.word
                && positioned_at(site.position, text, line, span, encoding, flavor)
        })
        .map(|site| site.id.clone())
}

fn positioned_at(
    position: Option<Position>,
    text: &str,
    line: u32,
    span: &WordSpan,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> bool {
    position.is_some_and(|position| {
        position.line == line.saturating_add(1)
            && encoded_column(text, position.column, encoding, flavor) == Some(span.start)
    })
}

fn performed_operation(
    index: &EffectIndex,
    text: &str,
    line: u32,
    span: &WordSpan,
    encoding: PositionEncoding,
) -> Option<OperationId> {
    let owner = dotted_owner(text, span, encoding)?;
    index
        .performs
        .iter()
        .find(|site| {
            site.position
                .is_some_and(|position| position.line == line.saturating_add(1))
                && site.id.effect == owner
                && site.id.operation == span.word
        })
        .map(|site| site.id.clone())
}

fn dotted_owner(text: &str, span: &WordSpan, encoding: PositionEncoding) -> Option<String> {
    let owner = trailing_path(prefix_to(text, span.start, encoding).strip_suffix('.')?);
    (!owner.is_empty()).then_some(owner)
}

/// The `a::b::` path `text` ends with, or empty when it ends with none.
fn trailing_path(text: &str) -> String {
    text.chars()
        .rev()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | ':'))
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn encoded_column(
    text: &str,
    column: u32,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> Option<u32> {
    let prefix = text.get(..byte_column(text, column, flavor)?)?;
    Some(measure(prefix, encoding))
}

/// The byte offset of an AST column: Default and Python count bytes, ML
/// characters.
fn byte_column(text: &str, column: u32, flavor: Flavor) -> Option<usize> {
    let column = usize::try_from(column).ok()?;
    match flavor {
        Flavor::Default | Flavor::Python => text.is_char_boundary(column).then_some(column),
        Flavor::Ml => text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
            .nth(column),
    }
}

fn effect_index(program: &Program) -> EffectIndex {
    let effect_symbols = collect_symbols(program)
        .into_iter()
        .filter(|symbol| symbol.ty == "effect")
        .collect();
    let mut index = EffectIndex {
        effect_symbols,
        ..EffectIndex::default()
    };
    walk_program(program, &mut index);
    index
}

impl AstVisitor for EffectIndex {
    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Effect {
                name,
                operations,
                doc,
                position,
                ..
            } => {
                let owner = self
                    .effect_symbols
                    .iter()
                    .find(|symbol| symbol.source_name == *name && symbol.position == *position)
                    .map_or(name.as_str(), |symbol| symbol.name.as_str());
                let declared = declarations(owner, operations, doc.as_ref());
                self.declarations.extend(declared);
            }
            Stmt::Signature { items, .. } => {
                self.contracts
                    .extend(items.iter().flat_map(|item| match item {
                        SignatureItem::Effect {
                            name, operations, ..
                        } => declarations(name, operations, None),
                        _ => Vec::new(),
                    }));
            }
            _ => {}
        }
    }

    fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Perform {
                effect,
                operation,
                position,
                ..
            } => self.performs.push(Site {
                id: operation_id(effect, operation),
                position: *position,
            }),
            Expr::Handler {
                effect,
                arms,
                position,
                ..
            } => {
                self.handlers.extend(arms.iter().map(|arm| Site {
                    id: operation_id(effect, &arm.operation),
                    position: arm.position,
                }));
                self.discharges.push(Discharge {
                    effect: base(effect).to_owned(),
                    position: *position,
                });
            }
            _ => {}
        }
    }
}

fn declarations(
    owner: &str,
    operations: &[EffectOperation],
    effect_doc: Option<&DocComment>,
) -> Vec<Declaration> {
    operations
        .iter()
        .map(|operation| Declaration {
            id: operation_id(owner, &operation.name),
            ty: operation.ty.clone(),
            // The operation's OWN doc wins. Falling back to the effect's doc
            // keeps undocumented operations useful, but showing the capability
            // blurb on every operation would make `Prompt.ask` and
            // `Prompt.tell` hover identically ([DOC-EFFECT-OP]).
            doc: operation
                .doc
                .as_ref()
                .or(effect_doc)
                .map(DocComment::render_markdown),
            position: operation.position,
        })
        .collect()
}

/// A discharge spells its instantiation (`Tensor<List<float>>`); identity is
/// the declaration it instantiates, so the mention is reduced to its base.
fn operation_id(effect: &str, operation: &str) -> OperationId {
    OperationId {
        effect: base(effect).to_owned(),
        operation: operation.to_owned(),
    }
}

mod implementations;

pub(crate) use implementations::{implementations, operation_definition};

#[cfg(test)]
mod tests;
