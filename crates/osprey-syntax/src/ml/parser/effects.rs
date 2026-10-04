//! ML effects parsing.
use super::{
    HandleHead, MlBinder, MlExpr, MlHandleArm, MlParam, Parser, Stage, TokKind,
    STATIC_STAGE_KEYWORD,
};

impl Parser<'_> {
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
        let pos = self.pos();
        self.advance(); // `perform`
        let first = self.ident().unwrap_or_default();
        let effect = self.instantiated_effect(first);
        if !self.eat(&TokKind::Dot) {
            self.error("expected '.' between effect and operation in perform");
        }
        let operation = self.operation_ident().unwrap_or_default();
        // `op ()` is a zero-argument performance, not application to unit.
        if matches!(self.peek(), TokKind::LParen) && matches!(self.peek_at(1), TokKind::RParen) {
            self.advance();
            self.advance();
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
            return_clause: head.return_clause,
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
        let mut return_clause = None;
        if self.eat(&TokKind::Indent) {
            while !self.at_block_end() {
                self.skip_separators();
                if self.at_block_end() {
                    break;
                }
                let before = self.i;
                let arm = self.handle_arm();
                if arm.operation == "return" {
                    if return_clause.is_some() || arm.params.len() != 1 {
                        self.error_at(
                            arm.pos,
                            "a handler permits one return clause with one parameter",
                        );
                    }
                    return_clause = Some(Box::new(MlExpr::Lambda {
                        params: arm.params.into_iter().map(MlParam::Named).collect(),
                        uncurried: true,
                        body: Box::new(arm.body),
                        pos: arm.pos,
                    }));
                } else {
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
            return_clause,
            pos,
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
        let mut params = Vec::new();
        while let TokKind::Ident(name) = self.peek() {
            params.push(MlBinder {
                name: name.clone(),
                pos: Some(self.pos()),
            });
            self.advance();
        }
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
        if matches!(self.peek(), TokKind::LParen) && matches!(self.peek_at(1), TokKind::RParen) {
            self.advance();
            self.advance();
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

    /// `select` + indented `pattern => body` arms — choose among ready channel
    /// arms, reusing the `match` arm grammar ([FLAVOR-ML-CONCURRENCY]).
    pub(in crate::ml) fn select_expr(&mut self) -> MlExpr {
        self.advance(); // `select`
        MlExpr::Select(self.match_arms_block())
    }
}
