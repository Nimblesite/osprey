//! ML `use` statements: `use p <- f (a)` passes the rest of its block to `f` as a
//! final callback, so the parser emits exactly the call and lambda the
//! hand-written pyramid would. Implements [BLOCK-USE], [FLAVOR-ML-USE], [BLOCK-USE-REJECT],
//! [LEX-USE-ARROW] and [FLAVOR-IR-EQUIV].
use super::{MlBinder, MlExpr, MlParam, Parser, Position, TokKind};

impl Parser<'_> {
    /// One `use` line and the rest of its block, as the call it stands for.
    pub(in crate::ml) fn use_line(&mut self) -> MlExpr {
        let pos = self.pos();
        self.advance(); // `use`
        let binders = self.use_binders();
        if !self.eat_use_arrow() {
            self.error("expected `<-` after the `use` binders");
        }
        let call = self.expr(0);
        self.skip_separators();
        let (items, value, value_pos) = self.block_items();
        if items.is_empty() && value.is_none() {
            self.error_at(pos, crate::USE_NEEDS_A_REST);
        }
        let callback = use_callback(
            binders,
            MlExpr::Block {
                items,
                value,
                pos: value_pos,
            },
            pos,
        );
        with_callback(call, callback).unwrap_or_else(|call| {
            self.error_at(pos, crate::USE_NEEDS_A_CALL);
            call
        })
    }

    /// The comma-separated names before `<-`; none for the binder-less form.
    fn use_binders(&mut self) -> Vec<MlBinder> {
        let mut binders = Vec::new();
        while let TokKind::Ident(name) = self.peek() {
            binders.push(MlBinder {
                name: name.clone(),
                pos: Some(self.pos()),
            });
            self.advance();
            if !self.eat(&TokKind::Comma) {
                break;
            }
        }
        binders
    }

    /// `<-` is two tokens to the lexer, so `x<-1` stays `x < -1` everywhere
    /// else; here they must touch.
    fn eat_use_arrow(&mut self) -> bool {
        let touching = matches!(self.peek(), TokKind::Op(op) if op == "<")
            && matches!(self.peek_at(1), TokKind::Op(op) if op == super::MINUS_OP)
            && self
                .toks
                .get(self.i)
                .zip(self.toks.get(self.i + 1))
                .is_some_and(|(lt, minus)| lt.range.end == minus.range.start);
        if touching {
            self.advance();
            self.advance();
        }
        touching
    }
}

/// The lambda a `use` passes: `\() =>`, `\p =>` or `\(p, q) =>`, exactly as the
/// parser reads those heads.
fn use_callback(binders: Vec<MlBinder>, body: MlExpr, pos: Position) -> MlExpr {
    let uncurried = binders.len() > 1;
    let params = if binders.is_empty() {
        vec![MlParam::Unit]
    } else {
        binders.into_iter().map(MlParam::Named).collect()
    };
    MlExpr::Lambda {
        params,
        uncurried,
        body: Box::new(body),
        pos,
    }
}

/// `call` with `callback` appended as its last argument, or the call back
/// unchanged when it is not a call.
fn with_callback(call: MlExpr, callback: MlExpr) -> Result<MlExpr, MlExpr> {
    match call {
        MlExpr::AppMulti { func, mut args } => {
            args.push(callback);
            Ok(MlExpr::AppMulti { func, args })
        }
        MlExpr::App { func, arg } => Ok(MlExpr::AppMulti {
            func,
            args: vec![unparen(*arg), callback],
        }),
        MlExpr::UnitApp { func } => Ok(MlExpr::App {
            func,
            arg: Box::new(callback),
        }),
        other => Err(other),
    }
}

/// `f (a)` is grouping, not a one-element list: the argument is `a`.
fn unparen(expr: MlExpr) -> MlExpr {
    match expr {
        MlExpr::Paren(inner) => *inner,
        other => other,
    }
}
