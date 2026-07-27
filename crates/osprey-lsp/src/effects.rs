//! Effect-operation hover and implementation navigation.
//!
//! Operations are identified by both their owning effect and operation name.
//! That keeps `Trace.mark` distinct from `Other.mark`, while one AST-derived
//! index serves declaration, `perform`, handler-arm hover, and implementation
//! queries. Implements [LSP-HOVER-EFFECT-OPERATIONS] and
//! [LSP-IMPLEMENTATIONS-EFFECT-HANDLERS].

use lspkit_vfs::PositionEncoding;
use osprey_ast::{walk_program, AstVisitor, Expr, Position, Program, Stmt};
use osprey_syntax::Flavor;

use crate::analysis::{collect_symbols, SymbolInfo};
use crate::features::nth_line;
use crate::model::Location;
use crate::text::{char_width, measure, prefix_to, word_at, WordSpan};
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

#[derive(Debug, Default)]
struct EffectIndex {
    declarations: Vec<Declaration>,
    performs: Vec<Site>,
    handlers: Vec<Site>,
    effect_symbols: Vec<SymbolInfo>,
}

/// Hover markdown for an operation declaration, `perform`, or handler arm.
pub(crate) fn operation_hover(
    program: &Program,
    source: &str,
    uri: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> Option<String> {
    let index = effect_index(program);
    let id = target_at(&index, source, line, character, encoding, flavor)?;
    let declaration = declaration(&index, uri, &id)?;
    Some(render_hover(&declaration, flavor))
}

/// Every handler arm implementing the operation under the cursor.
pub(crate) fn implementations(
    source: &str,
    uri: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
) -> Vec<Location> {
    let parsed = osprey_syntax::parse_program_for_path(uri, source);
    let index = effect_index(&parsed.program);
    let Some(id) = target_at(&index, source, line, character, encoding, parsed.flavor) else {
        return Vec::new();
    };
    let mut out = handler_locations(&index, source, uri, &id, encoding, parsed.flavor);
    for sibling in workspace::siblings(uri) {
        let flavor = crate::features::flavor_of(&sibling.uri, &sibling.source);
        let sibling_index = effect_index(&sibling.program);
        out.extend(handler_locations(
            &sibling_index,
            &sibling.source,
            &sibling.uri,
            &id,
            encoding,
            flavor,
        ));
    }
    out
}

fn declaration(index: &EffectIndex, uri: &str, id: &OperationId) -> Option<Declaration> {
    index
        .declarations
        .iter()
        .find(|declaration| same_operation(&declaration.id, id))
        .cloned()
        .or_else(|| {
            workspace::siblings(uri).into_iter().find_map(|sibling| {
                effect_index(&sibling.program)
                    .declarations
                    .into_iter()
                    .find(|declaration| same_operation(&declaration.id, id))
            })
        })
}

fn same_operation(left: &OperationId, right: &OperationId) -> bool {
    left.operation == right.operation
        && (left.effect == right.effect
            || qualified_suffix(&left.effect, &right.effect)
            || qualified_suffix(&right.effect, &left.effect))
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

fn handler_locations(
    index: &EffectIndex,
    source: &str,
    uri: &str,
    id: &OperationId,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> Vec<Location> {
    index
        .handlers
        .iter()
        .filter(|site| same_operation(&site.id, id))
        .filter_map(|site| site_location(site, source, uri, encoding, flavor))
        .collect()
}

fn site_location(
    site: &Site,
    source: &str,
    uri: &str,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> Option<Location> {
    let position = site.position?;
    let line = position.line.checked_sub(1)?;
    let text = nth_line(source, line)?;
    let start = encoded_column(text, position.column, encoding, flavor)?;
    let end = start.saturating_add(measure(&site.id.operation, encoding));
    Some(Location {
        uri: uri.to_owned(),
        span: (line, start, line, end),
    })
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
    let before = prefix_to(text, span.start, encoding).strip_suffix('.')?;
    let owner: String = before
        .chars()
        .rev()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | ':'))
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    (!owner.is_empty()).then_some(owner)
}

fn encoded_column(
    text: &str,
    column: u32,
    encoding: PositionEncoding,
    flavor: Flavor,
) -> Option<u32> {
    let column = usize::try_from(column).ok()?;
    match flavor {
        Flavor::Default => text.get(..column).map(|prefix| measure(prefix, encoding)),
        Flavor::Ml => Some(
            text.chars()
                .take(column)
                .map(|character| char_width(character, encoding))
                .sum(),
        ),
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
        let Stmt::Effect {
            name,
            operations,
            doc,
            position,
            ..
        } = statement
        else {
            return;
        };
        let owner = self
            .effect_symbols
            .iter()
            .find(|symbol| symbol.source_name == *name && symbol.position == *position)
            .map_or(name.as_str(), |symbol| symbol.name.as_str());
        self.declarations
            .extend(operations.iter().map(|operation| Declaration {
                id: operation_id(owner, &operation.name),
                ty: operation.ty.clone(),
                doc: doc.as_ref().map(osprey_ast::DocComment::render_markdown),
                position: operation.position,
            }));
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
            Expr::Handler { effect, arms, .. } => {
                self.handlers.extend(arms.iter().map(|arm| Site {
                    id: operation_id(effect, &arm.operation),
                    position: arm.position,
                }));
            }
            _ => {}
        }
    }
}

fn operation_id(effect: &str, operation: &str) -> OperationId {
    OperationId {
        effect: effect.to_owned(),
        operation: operation.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    mod unix {
        use super::super::*;

        const CREDIT_PERFORM_LINE: u32 = 90;
        const CREDIT_PERFORM_COLUMN: u32 = 52;
        const CREDIT_HANDLER_SPAN: (u32, u32, u32, u32) = (97, 8, 97, 14);

        fn route_document() -> (String, String) {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/projects/modules/src");
            let route = root.join("api/routes.ospml");
            let source = std::fs::read_to_string(&route).expect("read API routes");
            (source, format!("file://{}", route.display()))
        }

        fn credit_hover(source: &str, uri: &str) -> String {
            let parsed = osprey_syntax::parse_program_for_path(uri, source);
            operation_hover(
                &parsed.program,
                source,
                uri,
                CREDIT_PERFORM_LINE,
                CREDIT_PERFORM_COLUMN,
                PositionEncoding::Utf16,
                parsed.flavor,
            )
            .expect("resolve the Store declaration in the ledger sibling")
        }

        fn credit_implementations(source: &str, uri: &str) -> Vec<Location> {
            implementations(
                source,
                uri,
                CREDIT_PERFORM_LINE,
                CREDIT_PERFORM_COLUMN,
                PositionEncoding::Utf16,
            )
        }

        #[test]
        fn effect_navigation_crosses_project_files() {
            let (source, uri) = route_document();
            let hover = credit_hover(&source, &uri);
            assert!(hover.contains("bank::Ledger::Store.credit"), "{hover}");
            assert!(hover.contains("(int, int, string) => int"), "{hover}");
            let locations = credit_implementations(&source, &uri);
            assert_eq!(locations.len(), 1, "{locations:?}");
            let handler = locations.first().expect("one Store.credit handler");
            assert!(handler.uri.ends_with("src/main.ospml"), "{handler:?}");
            assert_eq!(handler.span, CREDIT_HANDLER_SPAN);
        }
    }
}
