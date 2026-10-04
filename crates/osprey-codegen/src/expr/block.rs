//! Function bodies share their frame scope; nested blocks own a lexical scope.
//! Implements [DEBUGGER-BLOCK-SCOPES].
use super::{gen_expr, Codegen, Expr, Position, Result, Stmt, Value};

pub(crate) fn gen_body(cg: &mut Codegen, body: &Expr) -> Result<Value> {
    let value = match body {
        Expr::Block {
            statements,
            value,
            position,
        } => gen_block_contents(cg, statements, value.as_deref(), *position, gen_expr),
        _ => gen_expr(cg, body),
    }?;
    let _ = cg.set_debug_position(crate::stmt::tail_position(body));
    Ok(value)
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
    cg.push_scope();
    let result = (|| {
        for (i, s) in statements.iter().enumerate() {
            crate::stmt::gen_local_stmt(cg, s)?;
            // Last-use drops: names the continuation no longer references die
            // here, not at function end [GC-ARC-PERCEUS].
            crate::arc::release_dead_after(cg, statements.get(i + 1..).unwrap_or(&[]), value);
        }
        let previous = cg.set_debug_position(position);
        let result = value.map_or_else(|| Ok(Value::unit()), |e| tail(cg, e));
        cg.restore_debug_position(previous);
        result
    })();
    cg.pop_scope();
    result
}
