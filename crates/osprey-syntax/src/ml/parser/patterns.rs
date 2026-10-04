//! ML patterns parsing.
use super::{
    is_constructor, MlArm, MlBinder, MlExpr, MlPattern, Parser, Position, TokKind, MINUS_OP,
};

impl Parser<'_> {
    /// `match scrutinee` + indented `pattern => body` arms.
    pub(in crate::ml) fn match_expr(&mut self) -> MlExpr {
        self.advance(); // `match`
        let scrutinee = self.expr(0);
        let arms = self.match_arms_block();
        MlExpr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        }
    }

    /// Parse an optional indented run of `pattern => body` arms shared by
    /// `match` and `select`.
    pub(in crate::ml) fn match_arms_block(&mut self) -> Vec<MlArm> {
        let mut arms = Vec::new();
        if self.eat(&TokKind::Indent) {
            while !self.at_block_end() {
                self.skip_separators();
                if self.at_block_end() {
                    break;
                }
                arms.push(self.match_arm());
            }
            let _ = self.eat(&TokKind::Dedent);
        }
        arms
    }

    pub(in crate::ml) fn match_arm(&mut self) -> MlArm {
        let pattern = self.pattern();
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in match arm");
        }
        let body = self.body_after_eq();
        MlArm { pattern, body }
    }

    /// A match pattern, plus the diagnostic for the or-pattern users reach for
    /// once `|` lexes: `|` separates union *variants*, never patterns
    /// ([FLAVOR-ML-UNION-INLINE]).
    pub(in crate::ml) fn pattern(&mut self) -> MlPattern {
        let pat = self.pattern_atom();
        if matches!(self.peek(), TokKind::Pipe) {
            self.error("or-patterns are not supported; write one arm per alternative");
            while self.eat(&TokKind::Pipe) {
                let _ = self.pattern_atom();
            }
        }
        pat
    }

    /// `( p )` groups a single pattern and is erased at parse time
    /// ([FLAVOR-ML-PATTERN-GROUP]); it is what lets a clause head write
    /// `check (Node l r)`. A comma list `(a, b)` is a tuple pattern, each slot
    /// a binder or `_` ([FLAVOR-ML-TUPLE], [PATTERN-TUPLE]).
    pub(in crate::ml) fn group_pattern(&mut self) -> MlPattern {
        self.advance(); // `(`
        let mut elements = vec![self.pattern()];
        while self.eat(&TokKind::Comma) {
            elements.push(self.pattern());
        }
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        match elements.pop() {
            Some(only) if elements.is_empty() => only,
            Some(last) => {
                elements.push(last);
                for slot in &elements {
                    if !matches!(slot, MlPattern::Bind(_) | MlPattern::Wildcard) {
                        self.error("a tuple pattern slot binds a name or `_`");
                    }
                }
                MlPattern::Tuple(elements)
            }
            None => MlPattern::Wildcard,
        }
    }

    /// `{ a, b }` / `{ a, .. }` — a structural row pattern: named binders,
    /// optionally opened by a trailing `..` ([PATTERN-STRUCTURAL]).
    pub(in crate::ml) fn structural_pattern(&mut self) -> MlPattern {
        self.advance(); // `{`
        let mut fields = Vec::new();
        let mut open = false;
        loop {
            if matches!(self.peek(), TokKind::Dot) && matches!(self.peek_at(1), TokKind::Dot) {
                self.advance();
                self.advance();
                open = true;
                break; // `..` is always the final entry
            }
            match self.peek().clone() {
                TokKind::Ident(name) => {
                    let pos = self.pos();
                    self.advance();
                    fields.push(MlBinder {
                        name,
                        pos: Some(pos),
                    });
                }
                _ => break,
            }
            if !self.eat(&TokKind::Comma) {
                break;
            }
        }
        if fields.is_empty() {
            // `{ .. }` would match every row and `{}` names nothing; a
            // structural pattern is closed-by-default over NAMED fields
            // ([PATTERN-STRUCTURAL]).
            self.error("a structural pattern names at least one field");
        }
        if !self.eat(&TokKind::RBrace) {
            self.error("expected '}'");
        }
        MlPattern::Structural { fields, open }
    }

    /// A match pattern: `_`, a literal, `Ctor field…`, `( p )`, or a bare
    /// binding.
    pub(in crate::ml) fn pattern_atom(&mut self) -> MlPattern {
        match self.peek().clone() {
            // `-N` — a negative integer literal pattern. The lexer splits this
            // into `-` then the magnitude, so fold the sign into the literal so
            // `-5` matches `-5`, mirroring the Default flavor ([FLAVOR-ML-MATCH]).
            TokKind::Op(op) if op == MINUS_OP && matches!(self.peek_at(1), TokKind::Int(_)) => {
                self.advance(); // `-`
                match self.peek().clone() {
                    TokKind::Int(n) => {
                        self.advance();
                        MlPattern::Int(-n)
                    }
                    _ => MlPattern::Wildcard,
                }
            }
            TokKind::Int(n) => {
                self.advance();
                MlPattern::Int(n)
            }
            TokKind::Str(raw) => {
                self.advance();
                MlPattern::Str(raw)
            }
            TokKind::KwTrue => {
                self.advance();
                MlPattern::Bool(true)
            }
            TokKind::KwFalse => {
                self.advance();
                MlPattern::Bool(false)
            }
            TokKind::Ident(name) => {
                let pos = self.pos();
                self.advance();
                self.ident_pattern(name, pos)
            }
            TokKind::LBracket => self.list_pattern(),
            TokKind::LParen => self.group_pattern(),
            TokKind::LBrace => self.structural_pattern(),
            other => {
                self.error(format!("unexpected token {other:?} in pattern"));
                MlPattern::Wildcard
            }
        }
    }

    /// `[ p, … ]` or `[ p, …, ...rest ]` — a list pattern with fixed-prefix
    /// element patterns and an optional trailing `...name` rest-binder
    /// ([FLAVOR-ML-MATCH], [TYPE-LIST-PATTERNS]). Layout is suppressed inside
    /// brackets, so elements may span lines.
    pub(in crate::ml) fn list_pattern(&mut self) -> MlPattern {
        self.advance(); // `[`
        let mut elements = Vec::new();
        let mut rest = None;
        if !matches!(self.peek(), TokKind::RBracket) {
            loop {
                if let Some(name) = self.rest_binder() {
                    rest = Some(name);
                    break; // `...rest` is always the final element
                }
                elements.push(self.pattern());
                if !self.more_in_list(&TokKind::RBracket) {
                    break;
                }
            }
        }
        if !self.eat(&TokKind::RBracket) {
            self.error("expected ']'");
        }
        MlPattern::List { elements, rest }
    }

    /// A `...name` rest-binder (three `.` tokens then an identifier), consumed
    /// only when it is actually present. Returns the bound name, or `None`.
    pub(in crate::ml) fn rest_binder(&mut self) -> Option<MlBinder> {
        let is_spread = matches!(self.peek(), TokKind::Dot)
            && matches!(self.peek_at(1), TokKind::Dot)
            && matches!(self.peek_at(2), TokKind::Dot);
        if !is_spread {
            return None;
        }
        self.advance();
        self.advance();
        self.advance();
        let pos = self.pos();
        self.ident().map(|name| MlBinder {
            name,
            pos: Some(pos),
        })
    }

    /// `_` → wildcard; `Ctor a b` → constructor binding payload fields; a bare
    /// lowercase name → a binding ([FLAVOR-ML-MATCH]).
    pub(in crate::ml) fn ident_pattern(&mut self, name: String, pos: Position) -> MlPattern {
        let name = self.qualified_name_tail(name);
        if name == "_" {
            return MlPattern::Wildcard;
        }
        if is_constructor(&name) {
            let mut fields = Vec::new();
            while let TokKind::Ident(field) = self.peek() {
                fields.push(MlBinder {
                    name: field.clone(),
                    pos: Some(self.pos()),
                });
                self.advance();
            }
            if matches!(self.peek(), TokKind::LParen) {
                self.error(
                    "nested constructor patterns are not supported; \
                     bind the payload and match it in a second expression",
                );
            }
            return MlPattern::Ctor { name, fields };
        }
        MlPattern::Bind(MlBinder {
            name,
            pos: Some(pos),
        })
    }
}
