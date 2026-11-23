//! Function bodies share their frame scope; nested blocks own a lexical scope.
//! Implements [DEBUGGER-BLOCK-SCOPES].
use super::{gen_expr, Codegen, Expr, Position, Result, Stmt, Value};

pub(crate) fn gen_body(cg: &mut Codegen, body: &Expr) -> Result<Value> {
    let value = if let Expr::Block {
        statements,
        value,
        position,
    } = body
    {
        gen_block_contents(cg, statements, value.as_deref(), *position, gen_expr)
    } else {
        gen_tail_body(cg, body)
    }?;
    let _ = cg.set_debug_position(crate::stmt::tail_position(body));
    Ok(value)
}

/// A body that is one expression is its own tail [GC-ARC-PERCEUS].
fn gen_tail_body(cg: &mut Codegen, body: &Expr) -> Result<Value> {
    crate::arc::scope_parameters(cg);
    crate::arc::begin_moves(cg, body, &std::collections::BTreeSet::new());
    let value = gen_expr(cg, body);
    crate::arc::end_moves(cg);
    value
}

pub(super) fn gen_block(
    cg: &mut Codegen,
    statements: &[Stmt],
    value: Option<&Expr>,
    position: Option<Position>,
) -> Result<Value> {
    gen_block_with(cg, statements, value, position, gen_expr)
}

pub(crate) fn gen_block_with(
    cg: &mut Codegen,
    statements: &[Stmt],
    value: Option<&Expr>,
    position: Option<Position>,
    tail: impl FnOnce(&mut Codegen, &Expr) -> Result<Value>,
) -> Result<Value> {
    let start = statements
        .first()
        .and_then(crate::stmt::stmt_position)
        .or(position);
    cg.with_debug_scope(start, |cg| {
        let result = gen_block_contents(cg, statements, value, position, tail)?;
        cg.mark_debug_block_exit(position)?;
        Ok(result)
    })
}

fn gen_block_contents(
    cg: &mut Codegen,
    statements: &[Stmt],
    value: Option<&Expr>,
    position: Option<Position>,
    tail: impl FnOnce(&mut Codegen, &Expr) -> Result<Value>,
) -> Result<Value> {
    // A child scope preserves outer bindings across nested blocks [BLOCK-SCOPE].
    cg.with_local_scope(|cg| {
        crate::arc::scope_parameters(cg);
        for (i, s) in statements.iter().enumerate() {
            let rest = statements.get(i + 1..).unwrap_or(&[]);
            crate::arc::begin_statement_moves(cg, s, rest, value.map(|tail| (tail, position)));
            let lowered = crate::stmt::gen_local_stmt(cg, s);
            crate::arc::end_moves(cg);
            lowered?;
            // Last-use drops: names the continuation no longer references die
            // here, not at function end [GC-ARC-PERCEUS].
            crate::arc::release_dead_after(cg, rest, value);
        }
        let previous = cg.set_debug_position(position);
        let result = value.map_or_else(
            || Ok(Value::unit()),
            |e| {
                // Nothing in this block is read after its tail [GC-ARC-PERCEUS].
                crate::arc::begin_moves(cg, e, &std::collections::BTreeSet::new());
                let result = tail(cg, e);
                crate::arc::end_moves(cg);
                result
            },
        );
        cg.restore_debug_position(previous);
        result
    })
}
