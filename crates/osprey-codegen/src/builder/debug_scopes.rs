//! Lexical source scopes for native variable visibility. [DEBUGGER-DBG-DECLARE]
use super::{Codegen, DebugState};
use crate::error::{CodegenError, Result};
use osprey_ast::Position;

impl DebugState {
    pub(super) fn begin_lexical_scope(&mut self, position: Option<Position>) -> Option<usize> {
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
    /// An identifier-only block tail still needs a source instruction in its scope.
    pub(crate) fn mark_debug_block_exit(&mut self, position: Option<Position>) -> Result<()> {
        if self
            .debug
            .as_ref()
            .and_then(|debug| debug.current_scope)
            .is_some()
        {
            self.hoist_debug_marker()?;
            let _ = self.set_debug_position(position);
            self.emit("store volatile i8 0, i8* %__osprey_debug_scope");
        }
        Ok(())
    }

    /// A fall-through branch disappears even at -O0. One stack byte per frame
    /// provides a real instruction without growing the stack in repeated blocks.
    fn hoist_debug_marker(&mut self) -> Result<()> {
        const SLOT: &str = "  %__osprey_debug_scope = alloca i8";
        if self.cur_lines.is_empty() {
            return Err(CodegenError::invalid("debug scope has no active function"));
        }
        if !self.cur_lines.iter().any(|line| line == SLOT) {
            self.cur_lines.insert(1, SLOT.to_string());
        }
        Ok(())
    }

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

impl Codegen {
    /// Set the current debug source position [DEBUGGER-SOURCE-MAP], returning
    /// the previous position.
    pub(crate) fn set_debug_position(&mut self, position: Option<Position>) -> Option<Position> {
        self.debug
            .as_mut()
            .and_then(|debug| debug.set_position(position))
    }

    /// Restore the debug source position captured by [`set_debug_position`].
    pub(crate) fn restore_debug_position(&mut self, previous: Option<Position>) {
        if let Some(debug) = self.debug.as_mut() {
            debug.current_position = previous;
        }
    }
}
