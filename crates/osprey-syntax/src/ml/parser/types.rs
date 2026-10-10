//! ML types parsing.
use super::{MlType, Parser, TokKind};

impl Parser<'_> {
    /// A type: arrows are right-associative (`a -> b -> c` = `a -> (b -> c)`).
    pub(in crate::ml) fn ty(&mut self) -> MlType {
        let from = self.ty_app();
        if self.eat(&TokKind::Arrow) {
            return MlType::Arrow {
                from: Box::new(from),
                to: Box::new(self.ty()),
            };
        }
        from
    }

    /// Type application `head arg…` — a head name applied to atom types.
    pub(in crate::ml) fn ty_app(&mut self) -> MlType {
        let head = self.ty_atom();
        let mut args = Vec::new();
        while self.starts_ty_atom() {
            args.push(self.ty_atom());
        }
        match head {
            MlType::Name(head) if !args.is_empty() => MlType::App { head, args },
            head => head,
        }
    }

    pub(in crate::ml) fn starts_ty_atom(&self) -> bool {
        matches!(self.peek(), TokKind::Ident(_) | TokKind::LParen)
    }

    /// A type atom: a name (optionally with `<…>` generic arguments), or a
    /// parenthesised group / tuple.
    pub(in crate::ml) fn ty_atom(&mut self) -> MlType {
        match self.peek().clone() {
            TokKind::Ident(name) => {
                self.advance();
                let name = self.qualified_name_tail(name);
                if self.at_angle_open() {
                    self.ty_generic_args(name)
                } else {
                    MlType::Name(name)
                }
            }
            TokKind::LParen => self.ty_paren(),
            other => {
                self.error(format!("unexpected token {other:?} in type"));
                MlType::Name("Unit".to_owned())
            }
        }
    }

    /// Whether the current token opens a generic argument list (`<`).
    pub(in crate::ml) fn at_angle_open(&self) -> bool {
        matches!(self.peek(), TokKind::Op(op) if op == "<")
    }

    /// `Head< t (, t)* >` — angle-bracketed generic arguments, lowered to the
    /// same [`MlType::App`] as the whitespace `Head t…` form so both render to
    /// `Head<…>` ([FLAVOR-ML-FN]). Reuses [`Self::ty`] for each argument.
    pub(in crate::ml) fn ty_generic_args(&mut self, head: String) -> MlType {
        self.advance(); // `<`
        let mut args = vec![self.ty()];
        while self.eat(&TokKind::Comma) {
            args.push(self.ty());
        }
        if matches!(self.peek(), TokKind::Op(op) if op == ">") {
            self.advance(); // `>`
        } else {
            self.error("expected '>' to close generic arguments");
        }
        MlType::App { head, args }
    }

    /// `( t )` grouping or `( t, t, … )` a tupled argument.
    pub(in crate::ml) fn ty_paren(&mut self) -> MlType {
        self.advance(); // `(`
        let mut parts = vec![self.ty()];
        while self.eat(&TokKind::Comma) {
            parts.push(self.ty());
        }
        let _ = self.eat(&TokKind::RParen);
        if parts.len() == 1 {
            parts
                .into_iter()
                .next()
                .unwrap_or(MlType::Name("Unit".to_owned()))
        } else {
            MlType::Tuple(parts)
        }
    }

    /// Whether the current `<` opens a construction-site type-argument list
    /// followed by an inline record (`Ctor<int>(field = v)`): scan a balanced
    /// `<…>` of type-shaped tokens, then require the `( Ident =` record
    /// opener — so a `Ctor < x` comparison never misparses.
    /// Whether a glued `<` opens **call-site type arguments** rather than a
    /// comparison. `<` alone cannot decide: `x<3` and `x<y` are ordinary
    /// comparisons, so committing on the bracket swallows them. The whole shape
    /// must be present first — a balanced angle run holding only tokens a type
    /// can hold, then the argument the application applies to, which is what
    /// makes `identity<int> 5` an application and `x<y` a comparison.
    /// Implements [TYPE-GENERICS-APPLY], [FLAVOR-ML-GENERICS].
    pub(in crate::ml) fn at_type_application(&self) -> bool {
        match self.past_angle_run() {
            Some(j) => self.starts_atom_at(j),
            None => false,
        }
    }

    /// Scan a `<…>` run that holds only tokens a TYPE can hold, and answer with
    /// the offset just past its closing `>`. `None` means the cursor is not on
    /// a `<`, or the run holds something no type can (`x<3`), or it never
    /// closes — in every one of those the `<` was an operator, and the caller
    /// must not commit to a generic form. Nested runs close with `>>`, which
    /// this flavor lexes as two `>` tokens, so counting depth is enough.
    pub(in crate::ml) fn past_angle_run(&self) -> Option<usize> {
        if !self.at_angle_open() {
            return None;
        }
        let mut depth = 0usize;
        let mut j = 0usize;
        loop {
            match self.peek_at(j) {
                TokKind::Op(op) if op == "<" => depth += 1,
                TokKind::Op(op) if op == ">" => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(j + 1);
                    }
                }
                TokKind::Ident(_)
                | TokKind::Comma
                | TokKind::Arrow
                | TokKind::LParen
                | TokKind::RParen => {}
                _ => return None,
            }
            j += 1;
        }
    }

    pub(in crate::ml) fn at_generic_record(&self) -> bool {
        let Some(j) = self.past_angle_run() else {
            return false;
        };
        matches!(self.peek_at(j), TokKind::LParen)
            && matches!(self.peek_at(j + 1), TokKind::Ident(_))
            && matches!(self.peek_at(j + 2), TokKind::Eq)
    }
}
