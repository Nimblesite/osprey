//! ML blocks parsing.
use super::{MlExpr, MlItem, MlParam, Parser, Position, TokKind};

impl Parser<'_> {
    /// `\param* => body` lambda. The juxtaposed head `\x y =>` is curried; the
    /// parenthesised comma-list head `\(x, y) =>` is uncurried ([FLAVOR-ML-CURRY]).
    pub(in crate::ml) fn lambda(&mut self) -> MlExpr {
        let pos = self.pos();
        self.advance(); // `\`
        let (params, uncurried) = self.head_params();
        // Clause heads are a *definition* form; a lambda has nowhere to put the
        // alternative arms ([FLAVOR-ML-CLAUSES]).
        if params.iter().any(|p| matches!(p, MlParam::Pattern(_))) {
            self.error_at(
                pos,
                "a lambda head takes plain parameters; use 'match' to select on a pattern",
            );
        }
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in lambda");
        }
        let body = self.body_after_eq();
        MlExpr::Lambda {
            params,
            uncurried,
            body: Box::new(body),
            pos,
        }
    }

    /// The body after `=`/`=>`: an inline expression, or an indented layout
    /// block whose trailing expression is its value ([FLAVOR-ML-BLOCK]).
    pub(in crate::ml) fn body_after_eq(&mut self) -> MlExpr {
        if !matches!(self.peek(), TokKind::Indent) {
            return self.inline_body();
        }
        self.advance(); // `Indent`
        let (items, value, pos) = self.block_items();
        let _ = self.eat(&TokKind::Dedent);
        MlExpr::Block { items, value, pos }
    }

    /// A body written on one line. `name := value` is an ITEM, not an
    /// expression, so `expr` can never reach it — yet [FLAVOR-ML-BIND] spells
    /// its own handler-arm example `tick => requests := (requests + 1) ?:
    /// requests`, and the indented spelling of that same arm already parses via
    /// `block_items`. Route the inline form through the same `assignment` item
    /// and wrap it in the one-item block the indented form produces, so both
    /// spellings lower to identical IR ([FLAVOR-IR-EQUIV]).
    pub(in crate::ml) fn inline_body(&mut self) -> MlExpr {
        if !matches!(self.peek(), TokKind::Ident(_)) || !matches!(self.peek_at(1), TokKind::ColonEq)
        {
            return self.expr(0);
        }
        match self.assignment() {
            Some(item) => MlExpr::Block {
                items: vec![item],
                value: None,
                pos: None,
            },
            None => self.expr(0),
        }
    }

    /// The items (and optional trailing value) of an indented block.
    pub(in crate::ml) fn block_items(
        &mut self,
    ) -> (Vec<MlItem>, Option<Box<MlExpr>>, Option<Position>) {
        let mut items = Vec::new();
        let mut value = None;
        let mut pos = None;
        while !self.at_block_end() {
            self.skip_separators();
            if self.at_block_end() {
                break;
            }
            let before = self.i;
            let start = self.pos();
            value = self.block_line(&mut items);
            if value.is_some() {
                pos = Some(start);
            }
            // Forward-progress guard ([FLAVOR-LOWER-CONTRACT]): a `block_line`
            // whose `item()` errored without consuming a token — a reserved word
            // (`do`/`effect`/…) or a malformed line inside the block — would
            // otherwise spin this loop forever. Recover past the offending token,
            // exactly as the top-level `program()` loop does, so any input
            // terminates.
            if self.i == before {
                self.recover();
            }
        }
        (items, value, pos)
    }

    /// Parse one block line. A trailing bare expression with nothing after it is
    /// the block value; anything else is appended as an item.
    pub(in crate::ml) fn block_line(&mut self, items: &mut Vec<MlItem>) -> Option<Box<MlExpr>> {
        if matches!(self.peek(), TokKind::KwHandle) {
            return Some(Box::new(self.handle_line()));
        }
        match self.item() {
            Some(MlItem::Expr { value, .. }) if self.at_block_end() => Some(Box::new(value)),
            Some(item) => {
                items.push(item);
                None
            }
            None => None,
        }
    }
}
