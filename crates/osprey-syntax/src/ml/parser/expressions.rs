//! ML expressions parsing.
use super::{
    infix_bp, record_head, MlExpr, MlSymbolPath, MlType, Parser, TokKind, ELVIS_OP, MINUS_OP,
};

impl Parser<'_> {
    /// Parse an expression whose operators bind at least as tightly as `min_bp`
    /// — the driving loop of Pratt / precedence climbing (Pratt 1973; Norvell).
    pub(in crate::ml) fn expr(&mut self, min_bp: u8) -> MlExpr {
        let mut left = self.unary();
        while let TokKind::Op(op) = self.peek() {
            let op = op.clone();
            let Some(bp) = infix_bp(&op) else { break };
            if bp < min_bp {
                break;
            }
            let pos = self.pos();
            self.advance();
            // `?:` is right-associative — recurse at its own binding power so
            // `f x ?: 0 ?: 1` groups as `f x ?: (0 ?: 1)`.
            let right = self.expr(if op == ELVIS_OP { bp } else { bp + 1 });
            left = MlExpr::Binary {
                pos,
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    /// A prefix unary (`-x`, `!x`) or an application.
    pub(in crate::ml) fn unary(&mut self) -> MlExpr {
        if let TokKind::Op(op) = self.peek() {
            if op == MINUS_OP && matches!(self.peek_at(1), TokKind::IntMinMagnitude) {
                // The one spelling of `i64::MIN`: its magnitude is a literal
                // only here ([`crate::I64_MIN_MAGNITUDE`]).
                self.advance();
                self.advance();
                return MlExpr::Int(i64::MIN);
            }
            if op == MINUS_OP || op == "!" {
                let op = op.clone();
                self.advance();
                let operand = self.unary();
                return MlExpr::Unary {
                    op,
                    operand: Box::new(operand),
                };
            }
        }
        self.application()
    }

    /// Whitespace application `f a b`, left-associative, recorded as nested
    /// single-argument [`MlExpr::App`] ([FLAVOR-ML-CALL]).
    pub(in crate::ml) fn application(&mut self) -> MlExpr {
        let pos = self.pos();
        let mut func = self.postfix();
        // `Head(field = v, …)` is an inline record literal, not application: any
        // identifier immediately followed by `(ident = …`. An UPPERCASE head is
        // construction (`Ctor(...)`); a LOWERCASE head is a non-destructive record
        // update (`receiver(...)`). Both lower to the same `MlExpr::Record` node —
        // and to the same canonical `Expr::TypeConstructor { name }` the Default
        // `Ctor { f: v }` / `receiver { f: v }` produce ([FLAVOR-ML-RECORD]).
        if let Some(name) = record_head(&func) {
            if self.at_inline_record() {
                func = self.inline_record(name, Vec::new());
            } else if self.at_generic_record() {
                // `Box<int>(item = 7)` — explicit construction-site type
                // arguments. Implements [TYPE-GENERICS-DECL],
                // [FLAVOR-ML-GENERICS].
                let type_args = match self.ty_generic_args(name.clone()) {
                    MlType::App { args, .. } => args,
                    _ => Vec::new(),
                };
                func = self.inline_record(name, type_args);
            }
        }
        if record_head(&func).is_some() && self.glued() && self.at_type_application() {
            let args = match self.ty_generic_args(String::new()) {
                MlType::App { args, .. } => args,
                _ => Vec::new(),
            };
            func = MlExpr::TypeApply {
                func: Box::new(func),
                args,
                pos,
            };
        }
        // `f ()` is a zero-argument application, not application to unit.
        if matches!(self.peek(), TokKind::LParen) && matches!(self.peek_at(1), TokKind::RParen) {
            self.advance();
            self.advance();
            func = MlExpr::UnitApp {
                func: Box::new(func),
            };
        }
        while self.starts_atom() || self.at_negative_literal_arg() {
            // `f (a, b)` — a parenthesised comma-list argument is the uncurried
            // saturated call: a single multi-argument `Call` ([FLAVOR-ML-CALL]).
            // A lone `f (a)` has no top-level comma and stays plain grouping.
            if matches!(self.peek(), TokKind::LParen) && self.first_paren_has_comma() {
                let args = self.uncurried_args();
                func = MlExpr::AppMulti {
                    func: Box::new(func),
                    args,
                };
                continue;
            }
            let arg = if self.at_negative_literal_arg() {
                self.negative_literal_arg()
            } else {
                self.postfix()
            };
            func = MlExpr::App {
                func: Box::new(func),
                arg: Box::new(arg),
            };
        }
        func
    }

    /// A `-` that opens a NEGATIVE LITERAL ARGUMENT rather than subtraction:
    /// spaced from what precedes it and glued to the digits that follow, so
    /// `gpuIota -3` applies `gpuIota` to `-3` — the parse its Default twin
    /// `gpuIota(-3)` has. `a - 3` (spaced both sides) and `a-3` (glued both
    /// sides) stay subtraction, so only the spelling that already reads as one
    /// argument changes meaning ([FLAVOR-ML-CALL], [FLAVOR-EQUIVALENCE]).
    pub(in crate::ml) fn at_negative_literal_arg(&self) -> bool {
        matches!(self.peek(), TokKind::Op(op) if op == MINUS_OP)
            && !self.glued()
            && self.glued_at(1)
            && matches!(self.peek_at(1), TokKind::Int(_) | TokKind::Float(_))
    }

    /// Consume `-<literal>` as one negated literal argument.
    pub(in crate::ml) fn negative_literal_arg(&mut self) -> MlExpr {
        self.advance(); // `-`
        MlExpr::Unary {
            op: String::from(MINUS_OP),
            operand: Box::new(self.postfix()),
        }
    }

    /// `( e ( , e )* )` — the parenthesised comma-list arguments of an uncurried
    /// saturated call, lowered to a single multi-argument `Call` ([FLAVOR-ML-CALL]).
    pub(in crate::ml) fn uncurried_args(&mut self) -> Vec<MlExpr> {
        self.advance(); // `(`
        let mut args = Vec::new();
        if !matches!(self.peek(), TokKind::RParen) {
            loop {
                args.push(self.expr(0));
                if !self.more_in_list(&TokKind::RParen) {
                    break;
                }
            }
        }
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        args
    }

    /// Postfix `.field` access and glued `[index]` chained onto an atom. A `[`
    /// only indexes when it abuts the target (`xs[0]`); a spaced `[` is a list
    /// literal argument, left for [`Self::application`] ([FLAVOR-ML-INDEX]).
    pub(in crate::ml) fn postfix(&mut self) -> MlExpr {
        let mut target = self.atom();
        loop {
            if self.eat(&TokKind::Dot) {
                if let Some(name) = self.ident() {
                    target = MlExpr::Field {
                        target: Box::new(target),
                        name,
                    };
                }
            } else if matches!(self.peek(), TokKind::LBracket) && self.glued() {
                target = self.index(target);
            } else {
                return target;
            }
        }
    }

    /// `target[index]` — consume a glued bracket index.
    pub(in crate::ml) fn index(&mut self, target: MlExpr) -> MlExpr {
        self.advance(); // `[`
        let index = self.expr(0);
        if !self.eat(&TokKind::RBracket) {
            self.error("expected ']'");
        }
        MlExpr::Index {
            target: Box::new(target),
            index: Box::new(index),
        }
    }

    /// Whether the current token abuts the previous one with no whitespace.
    pub(in crate::ml) fn glued(&self) -> bool {
        self.glued_at(0)
    }

    /// Whether the token `ahead` of the cursor abuts its predecessor.
    pub(in crate::ml) fn glued_at(&self, ahead: usize) -> bool {
        self.toks.get(self.i + ahead).is_some_and(|t| t.glued)
    }

    /// Whether the next token can begin an argument atom.
    pub(in crate::ml) fn starts_atom(&self) -> bool {
        self.starts_atom_at(0)
    }

    /// Whether the token `j` ahead of the cursor could begin an atom. Lookahead
    /// needs this to decide a shape before committing to it.
    pub(in crate::ml) fn starts_atom_at(&self, j: usize) -> bool {
        matches!(
            self.peek_at(j),
            TokKind::Int(_)
                | TokKind::Float(_)
                | TokKind::Str(_)
                | TokKind::Ident(_)
                | TokKind::KwTrue
                | TokKind::KwFalse
                | TokKind::LParen
                | TokKind::LBracket
                | TokKind::LBrace
        )
    }

    pub(in crate::ml) fn atom(&mut self) -> MlExpr {
        match self.peek().clone() {
            TokKind::Int(n) => {
                self.advance();
                MlExpr::Int(n)
            }
            // Reachable only where no unary minus precedes it, which is the
            // one position `i64::MIN`'s magnitude is not a literal.
            TokKind::IntMinMagnitude => {
                self.error(format!(
                    "invalid integer literal '{}'",
                    crate::I64_MIN_MAGNITUDE
                ));
                self.advance();
                MlExpr::Int(0)
            }
            TokKind::Float(f) => {
                self.advance();
                MlExpr::Float(f)
            }
            TokKind::KwTrue => {
                self.advance();
                MlExpr::Bool(true)
            }
            TokKind::KwFalse => {
                self.advance();
                MlExpr::Bool(false)
            }
            TokKind::Str(raw) => {
                let pos = self.pos();
                self.advance();
                MlExpr::Str { raw, pos }
            }
            TokKind::KwMatch => self.match_expr(),
            TokKind::KwSpawn => self.spawn_expr(),
            TokKind::KwPerform => self.perform_expr(),
            TokKind::KwHandler => self.handler_value_expr(),
            TokKind::KwResume => self.resume_expr(),
            TokKind::KwAwait => self.await_expr(),
            TokKind::KwYield => self.yield_expr(),
            TokKind::KwSend => self.send_expr(),
            TokKind::KwRecv => self.recv_expr(),
            TokKind::KwSelect => self.select_expr(),
            TokKind::Backslash => self.lambda(),
            TokKind::LParen => self.paren(),
            TokKind::LBracket => self.list(),
            TokKind::LBrace => self.anonymous_record(),
            // `kernel` opens a region only where an indented arm block follows;
            // everywhere else it is an ordinary name. Implements [STAGE-GPU-KERNEL].
            TokKind::Ident(_) if self.at_kernel_region() => self.kernel_expr(),
            TokKind::Ident(name) => {
                self.advance();
                self.ident_atom(name)
            }
            TokKind::Reserved(word) => {
                self.error(format!("ML construct '{word}' is not yet supported"));
                self.advance();
                MlExpr::Bool(false)
            }
            other => {
                self.error(format!("unexpected token {other:?} in expression"));
                self.advance();
                MlExpr::Bool(false)
            }
        }
    }

    /// An identifier atom: a bare reference, or — for an uppercase constructor
    /// directly followed by an indented `field = value` block — a record
    /// literal ([FLAVOR-ML-RECORD]).
    pub(in crate::ml) fn ident_atom(&mut self, name: String) -> MlExpr {
        let mut segments = vec![name];
        while self.eat(&TokKind::ColonColon) {
            if let Some(segment) = self.ident() {
                segments.push(segment);
            } else {
                self.error("expected path segment after '::'");
                break;
            }
        }
        if segments.len() > 1 {
            return MlExpr::Path(MlSymbolPath { segments });
        }
        let name = segments.pop().unwrap_or_default();
        if osprey_ast::is_record_constructor(&name, false) && matches!(self.peek(), TokKind::Indent)
        {
            let fields = self.record_fields();
            MlExpr::Record {
                name,
                type_args: Vec::new(),
                fields,
            }
        } else {
            MlExpr::Ident(name)
        }
    }
}
