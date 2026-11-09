//! Layout: indentation becomes block and newline tokens.

use crate::ml::token::{TokKind, Token};
use crate::SyntaxError;
use osprey_ast::Position;

/// Phase 2: insert `Indent`/`Dedent`/`Newline` from each line's first-token
/// column. Layout is suppressed while bracket depth is non-zero so a
/// parenthesised expression may span lines. Implements [FLAVOR-ML-LAYOUT].
pub(super) fn insert_layout(content: Vec<Token>) -> (Vec<Token>, Vec<SyntaxError>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut layout = Layout {
        stack: vec![0u32],
        regions: Vec::new(),
    };
    let mut depth = 0i32;
    let mut prev_line = 0u32;
    let mut after_arrow = false;
    let mut started = false;
    for tok in content {
        let fresh_line = tok.pos.line != prev_line;
        if fresh_line && (depth == 0 || layout.inside_region()) {
            emit_layout(&mut out, &mut layout.stack, &mut errors, tok.pos, started);
        } else if fresh_line && after_arrow && tok.pos.column > layout.top() {
            layout.open_region(depth, tok.pos, &mut out);
        }
        match tok.kind {
            TokKind::LParen | TokKind::LBracket | TokKind::LBrace => depth += 1,
            TokKind::RParen | TokKind::RBracket | TokKind::RBrace => {
                layout.close_regions_at(depth, tok.pos, &mut out);
                depth = (depth - 1).max(0);
            }
            _ => {}
        }
        after_arrow = matches!(tok.kind, TokKind::FatArrow);
        prev_line = tok.pos.line;
        out.push(tok);
        started = true;
    }
    let close = out.last().map_or(Position::default(), |t| t.pos);
    while layout.stack.last().copied().unwrap_or(0) > 0 {
        let _ = layout.stack.pop();
        out.push(layout_tok(TokKind::Dedent, close));
    }
    out.push(layout_tok(TokKind::Eof, close));
    (out, errors)
}

/// The offside state: the indentation stack, plus the body regions a lambda
/// opens INSIDE brackets.
///
/// Layout is suppressed inside brackets so a parenthesised expression may span
/// lines — but that also denied a lambda argument the indented body its
/// Default twin has, so `gpuMap (\x => …)` could not bind a local and no ML
/// twin of a block-bodied kernel was spellable ([GPU-KERNEL-FORM]). A `=>`
/// that ends its line re-opens layout for exactly that body, and the region
/// closes at its dedent or at the bracket that encloses it — whichever comes
/// first. A single-expression body still lowers to that expression
/// ([`super::lower`] collapses a statement-free block), so the older spelling
/// is untouched. Implements [FLAVOR-ML-LAYOUT].
struct Layout {
    stack: Vec<u32>,
    /// Per open region: the bracket depth it opened at, and the indentation
    /// stack length to unwind to when it closes.
    regions: Vec<(i32, usize)>,
}

impl Layout {
    fn inside_region(&self) -> bool {
        !self.regions.is_empty()
    }

    fn top(&self) -> u32 {
        self.stack.last().copied().unwrap_or(0)
    }

    /// Re-open layout for a lambda body written on the lines after its `=>`.
    fn open_region(&mut self, depth: i32, pos: Position, out: &mut Vec<Token>) {
        self.regions.push((depth, self.stack.len()));
        self.stack.push(pos.column);
        out.push(layout_tok(TokKind::Indent, pos));
    }

    /// Close every region opened at `depth` — the enclosing bracket is ending,
    /// so its body's block must end first.
    fn close_regions_at(&mut self, depth: i32, pos: Position, out: &mut Vec<Token>) {
        while self.regions.last().is_some_and(|(d, _)| *d == depth) {
            let unwind = self.regions.pop().map_or(0, |(_, len)| len);
            while self.stack.len() > unwind {
                let _ = self.stack.pop();
                out.push(layout_tok(TokKind::Dedent, pos));
            }
        }
    }
}

/// Compare a logical line's indentation against the stack, pushing one `Indent`,
/// a run of `Dedent`s, or a separating `Newline`.
fn emit_layout(
    out: &mut Vec<Token>,
    stack: &mut Vec<u32>,
    errors: &mut Vec<SyntaxError>,
    pos: Position,
    started: bool,
) {
    let col = pos.column;
    let top = stack.last().copied().unwrap_or(0);
    if col > top {
        stack.push(col);
        out.push(layout_tok(TokKind::Indent, pos));
        return;
    }
    while col < stack.last().copied().unwrap_or(0) {
        let _ = stack.pop();
        out.push(layout_tok(TokKind::Dedent, pos));
    }
    if col == stack.last().copied().unwrap_or(0) {
        if started {
            out.push(layout_tok(TokKind::Newline, pos));
        }
    } else {
        errors.push(SyntaxError {
            message: "inconsistent indentation does not match any enclosing block".to_owned(),
            position: pos,
        });
        stack.push(col);
        out.push(layout_tok(TokKind::Indent, pos));
    }
}

fn layout_tok(kind: TokKind, pos: Position) -> Token {
    // Synthetic layout markers never abut a content token meaningfully.
    Token {
        kind,
        pos,
        glued: false,
        range: 0..0,
    }
}
