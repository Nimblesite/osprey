//! `expectError` over a payload with no canonical rendering: a record, union
//! or collection error compares by content against the Result's own `E`.
//! Implements [TESTING-BUILTIN-EXPECT-ERROR] and [ERR-PAYLOAD].

use super::record_verdict;
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::Value;

/// Pass when `result` is an `Error` whose payload has the content of
/// `expected`. The payload is read only on the Error branch: a Success holds
/// none.
pub(super) fn expect_error_content(
    cg: &mut Codegen,
    result: &Value,
    expected: Value,
) -> Result<Value> {
    let ty = crate::result::error_type(result).ok_or_else(|| {
        CodegenError::unsupported(
            "expectError on a Result whose error type is not decided here; annotate it",
        )
    })?;
    let (success, error, end) = crate::result::open_result_branch(cg, result);
    let from_success = cg.snapshot_to(&end);
    cg.start_block(&error);
    let actual = crate::result::load_error_payload(cg, result);
    let same = crate::structural_equals::values_equal(cg, &ty, actual, expected)?;
    let from_error = cg.snapshot_to(&end);
    cg.start_block(&end);
    let _ = success;
    let ok = cg.emit_reg(format!(
        "phi i1 [ false, %{from_success} ], [ {same}, %{from_error} ]"
    ));
    let wanted = cg.string_constant(&format!("Error(<the given {ty}>)"));
    let got = cg.string_constant(&format!("a Success or an Error holding another {ty}"));
    Ok(record_verdict(cg, "null", &ok, &wanted, &got))
}
