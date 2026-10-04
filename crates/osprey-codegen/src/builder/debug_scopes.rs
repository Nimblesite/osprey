//! Lexical source scopes for native variable visibility. [DEBUGGER-DBG-DECLARE]
use super::{Codegen, DebugState};
use osprey_ast::Position;

impl DebugState {
    fn begin_lexical_scope(&mut self, position: Option<Position>) -> Option<usize> {
        let parent = self.current_scope?;
        let position = position.or(self.current_position)?;
        let id = self.alloc_id();
        let line = position.line.max(1);
        let column = position.column.saturating_add(1);
        self.dynamic.push((id, format!(
            "!{id} = distinct !DILexicalBlock(scope: !{parent}, file: !{}, line: {line}, column: {column})",
            self.file_id
        )));
        self.current_scope = Some(id);
        self.current_position = Some(position);
        Some(parent)
    }
}

impl Codegen {
    pub(crate) fn with_debug_scope<T>(
        &mut self,
        position: Option<Position>,
        emit: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self.debug.as_ref().and_then(|debug| debug.current_position);
        let parent = self
            .debug
            .as_mut()
            .and_then(|debug| debug.begin_lexical_scope(position));
        let result = emit(self);
        if let Some(debug) = self.debug.as_mut().filter(|_| parent.is_some()) {
            debug.current_scope = parent;
            debug.current_position = previous;
        }
        result
    }
}
