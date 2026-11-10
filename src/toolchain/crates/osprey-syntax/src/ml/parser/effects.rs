//! ML effects parsing.
use super::{
    HandleHead, MlClauses, MlExpr, MlHandleArm, MlParam, Parser, Stage, TokKind,
    STATIC_STAGE_KEYWORD,
};
use crate::ml::cst::{MlSelectArm, MlSelectOperation};

impl Parser<'_> {
    /// Whether the cursor stands on `()` and, if so, consumes it — the unit
    /// argument (`op ()`, `resume ()`), never application to unit.
    fn eat_unit_args(&mut self) -> bool {
        if matches!(self.peek(), TokKind::LParen) && matches!(self.peek_at(1), TokKind::RParen) {
            self.advance();
            self.advance();
            return true;
        }
        false
    }

    /// Where the current keyword stands, then the effect name following it,
    /// with any written instantiation, as `perform`, `mask` and `instance` all
    /// spell it ([FLAVOR-ML-EFFECT]).
    fn effect_head(&mut self) -> (osprey_ast::Position, String) {
        let pos = self.pos();
        self.advance();
        let head = self.ident().unwrap_or_default();
        (pos, self.instantiated_effect(head))
    }

    /// `spawn body` — start a fiber. The body is an indented layout block or an
    /// inline expression, parsed exactly like a `=`/`=>` body ([FLAVOR-ML-SPAWN]).
    pub(in crate::ml) fn spawn_expr(&mut self) -> MlExpr {
        self.advance(); // `spawn`
        MlExpr::Spawn(Box::new(self.body_after_eq()))
    }

    /// `perform Effect.op arg…` — perform an effect operation with
    /// whitespace-applied arguments ([FLAVOR-ML-EFFECT]). The head is the
    /// dotted `Effect.operation`; the trailing atoms are its arguments.
    pub(in crate::ml) fn perform_expr(&mut self) -> MlExpr {
        let (pos, effect) = self.effect_head();
        if !self.eat(&TokKind::Dot) {
            self.error("expected '.' between effect and operation in perform");
        }
        let operation = self.operation_ident().unwrap_or_default();
        // `op ()` is a zero-argument performance, not application to unit.
        if self.eat_unit_args() {
            return MlExpr::Perform {
                effect,
                operation,
                args: Vec::new(),
                pos,
            };
        }
        let mut args = Vec::new();
        while self.starts_atom() {
            args.push(self.postfix());
        }
        MlExpr::Perform {
            effect,
            operation,
            args,
            pos,
        }
    }

    /// `mask Effect` + an indented body. Implements [EFFECTS-MASK].
    pub(in crate::ml) fn mask_expr(&mut self) -> MlExpr {
        let (pos, effect) = self.effect_head();
        if !matches!(self.peek(), TokKind::Indent) {
            self.error("expected an indented body after `mask Effect`");
        }
        MlExpr::Mask {
            effect,
            body: Box::new(self.body_after_eq()),
            pos,
        }
    }

    /// `instance Effect` — a fresh capability. Implements [EFFECTS-INSTANCE].
    pub(in crate::ml) fn instance_expr(&mut self) -> MlExpr {
        let (pos, effect) = self.effect_head();
        MlExpr::Instance { effect, pos }
    }

    /// `handler Effect` + indented arms — the handler ITSELF, with no region
    /// attached: a value that can be bound, passed and called. Calling it with a
    /// zero-argument computation runs that computation under these arms.
    /// Implements [EFFECTS-HANDLER-VALUE].
    pub(in crate::ml) fn handler_value_expr(&mut self) -> MlExpr {
        let head = self.handle_head();
        MlExpr::HandlerValue {
            stage: head.stage,
            effect: head.effect,
            arms: head.arms,
            clauses: head.clauses,
            pos: head.pos,
        }
    }

    /// `handle Effect` and its arms, up to but not including the body.
    fn handle_head(&mut self) -> HandleHead {
        let pos = self.pos();
        self.advance(); // `handle`
                        // `handle static E` — the discharge half of [STAGE-DECL]. Contextual on
                        // the same terms as the declaration marker: only a following effect
                        // name makes `static` a stage rather than the handled effect's name.
        let stage = match (self.peek(), self.peek_at(1)) {
            (TokKind::Ident(word), TokKind::Ident(_)) if word == STATIC_STAGE_KEYWORD => {
                self.advance();
                Stage::Static
            }
            _ => Stage::Dynamic,
        };
        let first = self.ident().unwrap_or_default();
        let effect = self.instantiated_effect(first);
        let mut arms = Vec::new();
        let mut clauses = MlClauses::default();
        if self.eat(&TokKind::Indent) {
            while !self.at_block_end() {
                self.skip_separators();
                if self.at_block_end() {
                    break;
                }
                let before = self.i;
                let arm = self.handle_arm();
                if let Some(arm) = self.completion_clause(arm, &mut clauses) {
                    arms.push(arm);
                }
                if self.i == before {
                    self.recover();
                }
            }
            let _ = self.eat(&TokKind::Dedent);
        }
        HandleHead {
            stage,
            effect,
            arms,
            clauses,
            pos,
        }
    }

    /// File a `return x => …` or `finally => …` line into `clauses`; any other
    /// arm comes back to the caller. Each clause appears at most once; `return`
    /// takes one parameter and `finally` none. Implements
    /// [EFFECTS-HANDLER-ARMS] and [EFFECTS-FINALIZATION].
    fn completion_clause(
        &mut self,
        arm: MlHandleArm,
        clauses: &mut MlClauses,
    ) -> Option<MlHandleArm> {
        match arm.operation.as_str() {
            "return" => {
                if clauses.return_clause.is_some() || arm.params.len() != 1 {
                    self.error_at(
                        arm.pos,
                        "a handler permits one return clause with one parameter",
                    );
                }
                clauses.return_clause = Some(Box::new(MlExpr::Lambda {
                    params: arm.params.into_iter().map(MlParam::Named).collect(),
                    uncurried: true,
                    body: Box::new(arm.body),
                    pos: arm.pos,
                }));
                None
            }
            "finally" => {
                if clauses.finally.is_some() || !arm.params.is_empty() {
                    self.error_at(
                        arm.pos,
                        "a handler permits one finally clause with no parameter",
                    );
                }
                clauses.finally = Some(Box::new(arm.body));
                None
            }
            _ => Some(arm),
        }
    }

    /// A `handle` governs the remainder of its containing block.
    /// Implements [EFFECTS-HANDLE-REST].
    pub(in crate::ml) fn handle_line(&mut self) -> MlExpr {
        let head = self.handle_head();
        self.skip_separators();
        let (items, value, pos) = self.block_items();
        if items.is_empty() && value.is_none() {
            self.error_at(head.pos, crate::NOTHING_TO_HANDLE);
        }
        head.over(MlExpr::Block { items, value, pos })
    }

    /// The effect a request or region names, INCLUDING the instantiation when
    /// it wrote one. `Signal<Count>` and `Signal<Cursor>` are different effects
    /// to a row, so they are different effects to a handler, and both flavors
    /// spell the mention the same way. Implements [STAGE-SIGNALS-EXACT].
    pub(in crate::ml) fn instantiated_effect(&mut self, first: String) -> String {
        let name = self.qualified_name_tail(first);
        if !self.at_angle_open() {
            return name;
        }
        let applied = self.ty_generic_args(name);
        crate::ml::lower::render_type(&applied)
    }

    /// The parser's own cursor, so a sibling module can tell whether an arm
    /// consumed anything and recover when it did not.
    pub(in crate::ml) fn position_index(&self) -> usize {
        self.i
    }

    /// One `op param* => body` arm of a `handle` expression.
    pub(in crate::ml) fn handle_arm(&mut self) -> MlHandleArm {
        let pos = self.pos();
        let operation = self.operation_ident().unwrap_or_default();
        let params = self.binder_run();
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in handle arm");
        }
        let body = self.body_after_eq();
        MlHandleArm {
            operation,
            params,
            body,
            pos,
        }
    }

    /// Explicit application invokes the continuation. Bare `resume` denotes
    /// an owned value and must never silently invoke it with Unit.
    pub(in crate::ml) fn resume_expr(&mut self) -> MlExpr {
        self.advance(); // `resume`
                        // `resume ()` is a unit resume, like the Default `resume()`.
        if self.eat_unit_args() {
            return MlExpr::Resume(None);
        }
        // An indented block, or a `match` whose arms are the resumed value,
        // supplies the whole remaining body.
        if matches!(self.peek(), TokKind::Indent | TokKind::KwMatch) {
            return MlExpr::Resume(Some(Box::new(self.body_after_eq())));
        }
        // Otherwise `resume` is an ordinary application and takes ONE argument.
        // Reading the rest of the line instead made `resume cap + 1` mean
        // `resume (cap + 1)`, a different program from its Default twin
        // `resume(cap) + 1` ([FLAVOR-IR-EQUIV]).
        if self.starts_atom() {
            return MlExpr::Resume(Some(Box::new(self.postfix())));
        }
        self.error(
            "owned continuation values are not implemented; use `resume ()` to invoke with Unit",
        );
        MlExpr::Resume(None)
    }

    /// `await fiber` — block on a spawned fiber. Takes one postfix atom (the
    /// fiber handle), so `await (spawn f x)` nests via the parenthesised group
    /// ([FLAVOR-ML-CONCURRENCY]).
    pub(in crate::ml) fn await_expr(&mut self) -> MlExpr {
        self.advance(); // `await`
        MlExpr::Await(Box::new(self.postfix()))
    }

    /// `yield` or `yield value` — yield from the current fiber. A bare `yield`
    /// (nothing more on the line) yields unit ([FLAVOR-ML-CONCURRENCY]).
    pub(in crate::ml) fn yield_expr(&mut self) -> MlExpr {
        self.advance(); // `yield`
        if self.starts_atom() {
            return MlExpr::Yield(Some(Box::new(self.postfix())));
        }
        MlExpr::Yield(None)
    }

    /// `send channel value` — send a value on a channel; channel and value are
    /// each one postfix atom ([FLAVOR-ML-CONCURRENCY]).
    pub(in crate::ml) fn send_expr(&mut self) -> MlExpr {
        self.advance(); // `send`
        let channel = Box::new(self.postfix());
        let value = Box::new(self.postfix());
        MlExpr::Send { channel, value }
    }

    /// `recv channel` — receive a value from a channel ([FLAVOR-ML-CONCURRENCY]).
    pub(in crate::ml) fn recv_expr(&mut self) -> MlExpr {
        self.advance(); // `recv`
        MlExpr::Recv(Box::new(self.postfix()))
    }

    /// `select` + indented arms: `name = recv c`, `recv c`, `send c v`, `timeout ms`
    /// or `_`, each `=> body` ([FLAVOR-ML-CONCURRENCY], [CONCURRENCY-SELECT]).
    pub(in crate::ml) fn select_expr(&mut self) -> MlExpr {
        self.advance(); // `select`
        let mut arms = Vec::new();
        if self.eat(&TokKind::Indent) {
            while !self.at_block_end() {
                self.skip_separators();
                if self.at_block_end() {
                    break;
                }
                arms.push(self.select_arm());
            }
            let _ = self.eat(&TokKind::Dedent);
        }
        MlExpr::Select(arms)
    }

    fn select_arm(&mut self) -> MlSelectArm {
        let pos = self.pos();
        let (binding, operation) = self.select_head();
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in select arm");
        }
        MlSelectArm {
            binding,
            operation,
            body: self.body_after_eq(),
            pos,
        }
    }

    fn select_head(&mut self) -> (Option<String>, MlSelectOperation) {
        let name = match self.peek() {
            TokKind::Ident(name) => Some(name.clone()),
            _ => None,
        };
        match (self.peek(), self.peek_at(1)) {
            (TokKind::KwRecv, _) => (None, self.select_recv()),
            (TokKind::KwSend, _) => {
                self.advance();
                let channel = self.postfix();
                let value = self.postfix();
                (None, MlSelectOperation::Send(channel, value))
            }
            (TokKind::Ident(_), TokKind::Eq) => {
                self.advance();
                self.advance();
                (name.filter(|name| name != "_"), self.select_recv())
            }
            _ if name.as_deref() == Some("_") => {
                self.advance();
                (None, MlSelectOperation::Default)
            }
            _ if name.as_deref() == Some("timeout") => {
                self.advance();
                (None, MlSelectOperation::Timeout(self.postfix()))
            }
            _ => {
                self.error("a select arm is `name = recv channel`, `recv channel`, `send channel value`, `timeout milliseconds` or `_`");
                (None, MlSelectOperation::Default)
            }
        }
    }

    fn select_recv(&mut self) -> MlSelectOperation {
        if !self.eat(&TokKind::KwRecv) {
            self.error("expected `recv` after `=` in select arm");
        }
        MlSelectOperation::Recv(self.postfix())
    }
}
