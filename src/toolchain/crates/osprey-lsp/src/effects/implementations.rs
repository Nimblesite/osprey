//! Navigation over the effect index, in the open buffer and then its project
//! siblings. `textDocument/implementation` lists the handler arms that answer
//! an operation and the handlers that discharge an effect
//! ([LSP-IMPLEMENTATIONS-EFFECT-HANDLERS]); `textDocument/definition` lands on
//! the operation's declaration ([LSP-DEFINITION-EFFECT-OPERATIONS]).

use lspkit::vfs::PositionEncoding;
use osprey_syntax::Flavor;

use osprey_ast::Position;

use super::{
    byte_column, effect_index, encoded_column, same_effect, same_operation, target_at,
    trailing_path, Discharge, EffectIndex, OperationId,
};
use crate::features::nth_line;
use crate::model::Location;
use crate::text::{measure, prefix_to, word_at};
use crate::workspace;

/// What an implementation query asks about: one operation, or a whole effect.
enum Target {
    Operation(OperationId),
    Effect(String),
}

/// One indexed file of the project; the open buffer comes first.
struct IndexedFile<'a> {
    index: EffectIndex,
    source: &'a str,
    uri: &'a str,
    flavor: Flavor,
}

/// Every handler arm implementing the operation under the cursor, or every
/// handler discharging the effect under the cursor.
pub(crate) fn implementations(
    source: &str,
    uri: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
    project: &workspace::View,
) -> Vec<Location> {
    let files = indexed_files(source, uri, project);
    implementation_target(&files, line, character, encoding).map_or_else(Vec::new, |target| {
        files
            .iter()
            .flat_map(|file| file.implementations(&target, encoding))
            .collect()
    })
}

/// The declaration of the operation under the cursor, from its declaration, a
/// handler arm or a `perform`. Siblings are indexed only once the cursor is on
/// an operation, and the first declaring file wins, so a file two manifests
/// share answers once. Implements [LSP-DEFINITION-EFFECT-OPERATIONS].
pub(crate) fn operation_definition(
    source: &str,
    uri: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
    project: &workspace::View,
) -> Vec<Location> {
    let current = current_file(source, uri, project);
    current
        .operation_at(line, character, encoding)
        .and_then(|id| {
            std::iter::once(current)
                .chain(sibling_files(project))
                .find_map(|file| file.declaration(&id, encoding))
        })
        .into_iter()
        .collect()
}

/// The open buffer, then every project sibling ([LSP-WORKSPACE]).
fn indexed_files<'a>(
    source: &'a str,
    uri: &'a str,
    project: &'a workspace::View,
) -> Vec<IndexedFile<'a>> {
    std::iter::once(current_file(source, uri, project))
        .chain(sibling_files(project))
        .collect()
}

fn current_file<'a>(source: &'a str, uri: &'a str, project: &workspace::View) -> IndexedFile<'a> {
    IndexedFile {
        index: effect_index(&project.program(uri, source)),
        source,
        uri,
        flavor: project.flavor(uri, source),
    }
}

fn sibling_files(project: &workspace::View) -> impl Iterator<Item = IndexedFile<'_>> {
    project.siblings.iter().map(|sibling| IndexedFile {
        index: effect_index(&sibling.program),
        source: &sibling.source,
        uri: &sibling.uri,
        flavor: sibling.flavor,
    })
}

fn implementation_target(
    files: &[IndexedFile<'_>],
    line: u32,
    character: u32,
    encoding: PositionEncoding,
) -> Option<Target> {
    files
        .first()?
        .operation_at(line, character, encoding)
        .map(Target::Operation)
        .or_else(|| effect_at(files, line, character, encoding).map(Target::Effect))
}

/// The effect named under the cursor — at its declaration, a discharge, a
/// `perform` qualifier or anywhere else — when some project file declares or
/// discharges it. A builtin such as `Arith` has no declaration to find.
fn effect_at(
    files: &[IndexedFile<'_>],
    line: u32,
    character: u32,
    encoding: PositionEncoding,
) -> Option<String> {
    let text = nth_line(files.first()?.source, line)?;
    let span = word_at(text, character, encoding)?;
    let mention = trailing_path(prefix_to(text, span.start, encoding)) + &span.word;
    files
        .iter()
        .flat_map(|file| {
            let declared = file.index.effect_symbols.iter().map(|symbol| &symbol.name);
            declared.chain(file.index.discharges.iter().map(|site| &site.effect))
        })
        .any(|effect| same_effect(effect, &mention))
        .then_some(mention)
}

impl IndexedFile<'_> {
    /// The operation named at `(line, character)` of this file.
    fn operation_at(
        &self,
        line: u32,
        character: u32,
        encoding: PositionEncoding,
    ) -> Option<OperationId> {
        target_at(
            &self.index,
            self.source,
            line,
            character,
            encoding,
            self.flavor,
        )
    }

    fn implementations(&self, target: &Target, encoding: PositionEncoding) -> Vec<Location> {
        match target {
            Target::Operation(id) => self
                .index
                .handlers
                .iter()
                .filter(|site| same_operation(&site.id, id))
                .filter_map(|site| self.named_at(site.position, &site.id.operation, encoding))
                .collect(),
            Target::Effect(effect) => self
                .index
                .discharges
                .iter()
                .filter(|discharge| same_effect(&discharge.effect, effect))
                .filter_map(|discharge| self.discharge_location(discharge, encoding))
                .collect(),
        }
    }

    fn declaration(&self, id: &OperationId, encoding: PositionEncoding) -> Option<Location> {
        self.index
            .declarations
            .iter()
            .find(|declaration| same_operation(&declaration.id, id))
            .and_then(|declaration| self.named_at(declaration.position, &id.operation, encoding))
    }

    /// The range of `name` starting at an AST `position`.
    fn named_at(
        &self,
        position: Option<Position>,
        name: &str,
        encoding: PositionEncoding,
    ) -> Option<Location> {
        let position = position?;
        let line = position.line.checked_sub(1)?;
        let text = nth_line(self.source, line)?;
        let start = encoded_column(text, position.column, encoding, self.flavor)?;
        Some(Location {
            uri: self.uri.to_owned(),
            span: (
                line,
                start,
                line,
                start.saturating_add(measure(name, encoding)),
            ),
        })
    }

    /// The effect's name in the handler head, searched from the `handle`
    /// keyword; a name on a later line lands on the keyword itself.
    fn discharge_location(
        &self,
        discharge: &Discharge,
        encoding: PositionEncoding,
    ) -> Option<Location> {
        let position = discharge.position?;
        let line = position.line.checked_sub(1)?;
        let text = nth_line(self.source, line)?;
        let keyword = byte_column(text, position.column, self.flavor)?;
        let name = text
            .get(keyword..)?
            .find(&discharge.effect)
            .map_or(keyword, |offset| keyword.saturating_add(offset));
        let start = measure(text.get(..name)?, encoding);
        Some(Location {
            uri: self.uri.to_owned(),
            span: (
                line,
                start,
                line,
                start.saturating_add(measure(&discharge.effect, encoding)),
            ),
        })
    }
}
