//! ML parameters parsing.
use super::{is_constructor, MlBinder, MlParam, Parser, TokKind, Token, MINUS_OP};

impl Parser<'_> {
    /// The parameter list of a binding or lambda head, plus whether it was the
    /// **uncurried** parenthesised comma-list form `(x, y)` (→ a flat
    /// multi-parameter function/lambda) rather than the juxtaposed curried form
    /// `x y` (→ a nested-lambda chain) ([FLAVOR-ML-CURRY]). The uncurried form is
    /// a single parenthesised group holding a top-level comma; everything else
    /// (juxtaposed names, a lone `(x)` / `(x : t)` / `()`) is curried.
    pub(in crate::ml) fn head_params(&mut self) -> (Vec<MlParam>, bool) {
        if matches!(self.peek(), TokKind::LParen) && self.first_paren_has_comma() {
            (self.uncurried_params(), true)
        } else {
            (self.params(), false)
        }
    }

    /// Collect zero or more juxtaposed surface parameter patterns up to the
    /// `=`/`=>` — the curried head form.
    pub(in crate::ml) fn params(&mut self) -> Vec<MlParam> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                // An uppercase head in parameter position is a nullary
                // constructor pattern, not a binder ([FLAVOR-ML-CLAUSES]).
                TokKind::Ident(name) if is_constructor(name) => {
                    out.push(MlParam::Pattern(self.pattern()));
                }
                TokKind::Ident(name) => {
                    let name = name.clone();
                    self.advance();
                    out.push(MlParam::Named(MlBinder {
                        name,
                        pos: self.toks.get(self.i.saturating_sub(1)).map(|t| t.pos),
                    }));
                }
                TokKind::LParen if self.at_pattern_param() => {
                    out.push(MlParam::Pattern(self.pattern()));
                }
                TokKind::LBracket => out.push(MlParam::Pattern(self.pattern())),
                TokKind::LParen => out.push(self.paren_param()),
                TokKind::Int(_) | TokKind::Str(_) | TokKind::KwTrue | TokKind::KwFalse => {
                    out.push(MlParam::Pattern(self.pattern()));
                }
                TokKind::Op(op) if op == MINUS_OP && matches!(self.peek_at(1), TokKind::Int(_)) => {
                    out.push(MlParam::Pattern(self.pattern()));
                }
                _ => break,
            }
        }
        out
    }

    /// Whether the `(` here opens a grouped clause pattern (`(Node l r)`,
    /// `(-1)`) rather than a parameter binder (`(x)`, `(x : int)`, `()`).
    pub(in crate::ml) fn at_pattern_param(&self) -> bool {
        match self.peek_at(1) {
            TokKind::Ident(name) => is_constructor(name),
            TokKind::Int(_) | TokKind::Str(_) | TokKind::KwTrue | TokKind::KwFalse => true,
            TokKind::Op(op) => op == MINUS_OP,
            _ => false,
        }
    }

    /// `( p ( , p )* )` — the parenthesised comma-list parameters of the
    /// uncurried head form ([FLAVOR-ML-CURRY]).
    pub(in crate::ml) fn uncurried_params(&mut self) -> Vec<MlParam> {
        self.advance(); // `(`
        let mut out = Vec::new();
        if !matches!(self.peek(), TokKind::RParen) {
            loop {
                out.push(self.one_param());
                if !self.more_in_list(&TokKind::RParen) {
                    break;
                }
            }
        }
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        out
    }

    /// A parenthesised parameter: `()` (the unit marker), `(name)`, or the inline
    /// type-annotated `(name : type)` a lambda uses for a load-bearing parameter
    /// type ([FLAVOR-ML-FN]).
    pub(in crate::ml) fn paren_param(&mut self) -> MlParam {
        self.advance(); // `(`
        let param = self.one_param();
        let _ = self.eat(&TokKind::RParen);
        param
    }

    /// One parameter inside a `(…)` group: a named `name`, a type-annotated
    /// `name : type`, or the unit marker (no name). Shared by the lone `(x)` and
    /// the comma-list `(x, y)` forms so neither duplicates the rule.
    pub(in crate::ml) fn one_param(&mut self) -> MlParam {
        match self.peek() {
            TokKind::Ident(name) => {
                let binder_pos = self.pos();
                let name = name.clone();
                self.advance();
                let start = self.i;
                if self.eat(&TokKind::Colon) {
                    let pos = self.pos();
                    let ty = self.ty();
                    crate::ml::annotation_edits::parameter(
                        self.source,
                        self.toks,
                        start,
                        self.i,
                        &name,
                        self.annotations,
                    );
                    MlParam::Typed(
                        MlBinder {
                            name,
                            pos: Some(binder_pos),
                        },
                        ty,
                        pos,
                    )
                } else {
                    MlParam::Named(MlBinder {
                        name,
                        pos: Some(binder_pos),
                    })
                }
            }
            _ => MlParam::Unit,
        }
    }

    /// Non-consuming: does the parenthesised group opening at the current `(`
    /// hold a top-level comma before its matching `)`? Distinguishes the
    /// uncurried comma-list `(x, y)` from grouping `(x)` and the unit `()`.
    pub(in crate::ml) fn first_paren_has_comma(&self) -> bool {
        self.group_has_at_top_level(&TokKind::Comma)
    }

    /// Non-consuming: does `marker` appear at the OWN nesting depth of the
    /// group opening at the cursor, before that group closes? `(` and `[` both
    /// open a level, so a marker nested inside an inner group cannot spoof one
    /// at the top level. False at end of input or when the group closes first.
    pub(in crate::ml) fn group_has_at_top_level(&self, marker: &TokKind) -> bool {
        let mut depth = 0i32;
        let mut j = self.i;
        while let Some(tok) = self.toks.get(j) {
            match tok.kind {
                TokKind::LParen | TokKind::LBracket => depth += 1,
                TokKind::RParen | TokKind::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                TokKind::Eof => return false,
                ref kind if depth == 1 && kind == marker => return true,
                _ => {}
            }
            j += 1;
        }
        false
    }

    /// Lookahead (non-consuming): does the run from the current identifier end
    /// in `=` on this logical line (`Ident headAtom* =`)? A head atom is a
    /// binder, a literal, or a bracketed group — the clause forms
    /// ([FLAVOR-ML-CLAUSES]). Operators are deliberately absent, so `f 1 == 2`
    /// stays an expression.
    pub(in crate::ml) fn is_binding_head(&self) -> bool {
        let mut j = self.i + 1; // past the leading identifier
        loop {
            match self.toks.get(j).map(|t| &t.kind) {
                Some(
                    TokKind::Ident(_)
                    | TokKind::Int(_)
                    | TokKind::Str(_)
                    | TokKind::KwTrue
                    | TokKind::KwFalse,
                ) => j += 1,
                Some(TokKind::Op(op)) if op == MINUS_OP => j += 1,
                Some(TokKind::LParen) => j = Self::past_group(self.toks, j, &TokKind::RParen),
                Some(TokKind::LBracket) => j = Self::past_group(self.toks, j, &TokKind::RBracket),
                Some(TokKind::Eq) => return true,
                _ => return false,
            }
        }
    }

    /// Index just past the bracketed group opening at `open`, scanning to its
    /// `close` token. Nesting is not tracked: a head atom's group holds a
    /// pattern, which cannot itself contain a bracket in this flavor.
    pub(in crate::ml) fn past_group(toks: &[Token], open: usize, close: &TokKind) -> usize {
        let mut j = open + 1;
        while !matches!(toks.get(j).map(|t| &t.kind), Some(TokKind::Eof) | None) {
            if toks.get(j).map(|t| &t.kind) == Some(close) {
                return j + 1;
            }
            j += 1;
        }
        j
    }
}
