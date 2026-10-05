//! Native HTTP response record ABI. [TYPE-RECORD-C-ABI]
use super::{gen_expr, Codegen, CodegenError, FieldAssignment, Result, Value};

/// The built-in HTTP response record name.
pub(crate) const HTTP_RESPONSE: &str = "HttpResponse";

/// `{ i64 status, i8* headers, i8* contentType, i64 streamFd, i8 isComplete,
/// i8* partialBody }` — the C `struct HttpResponse` (`runtime/http_shared.h`),
/// the one record returned across the FFI boundary. Field LLVM types in layout
/// order; `isComplete` is the C `bool`, an `i8`.
pub(crate) const HTTP_RESPONSE_STRUCT: &str = "{ i64, i8*, i8*, i64, i8, i8* }";
const HTTP_RESPONSE_FIELDS: [(&str, &str); 6] = [
    ("status", "i64"),
    ("headers", "i8*"),
    ("contentType", "i8*"),
    ("streamFd", "i64"),
    ("isComplete", "i8"),
    ("partialBody", "i8*"),
];

pub(super) fn layout_meta() -> i64 {
    crate::meta::struct_meta(
        &HTTP_RESPONSE_FIELDS.map(|(_, ty)| crate::meta::MetaField::of_slot_ty(ty)),
    )
}

pub(super) fn gen_http_response(cg: &mut Codegen, fields: &[FieldAssignment]) -> Result<Value> {
    let layout = cg
        .ctor_layout(HTTP_RESPONSE)
        .ok_or_else(|| CodegenError::unknown(HTTP_RESPONSE))?;
    let obj = cg.malloc_struct_noinit(HTTP_RESPONSE_STRUCT, layout_meta());
    for (index, (name, ty)) in layout.fields.iter().enumerate() {
        let field = fields
            .iter()
            .find(|field| &field.name == name)
            .ok_or_else(|| {
                CodegenError::invalid(format!("missing field `{name}` for `{HTTP_RESPONSE}`"))
            })?;
        let value = gen_expr(cg, &field.value)?;
        let value = crate::cast::coerce_to(cg, value, *ty)?;
        super::store_record_field(
            cg,
            HTTP_RESPONSE,
            HTTP_RESPONSE_STRUCT,
            &obj,
            index,
            *ty,
            &value.operand,
        );
    }
    Ok(super::own_struct_handle(
        cg,
        HTTP_RESPONSE_STRUCT,
        &obj,
        HTTP_RESPONSE,
    ))
}

pub(super) fn load_bool(cg: &mut Codegen, ty: &str, source: &str, index: usize) -> String {
    let slot = cg.emit_reg(format!(
        "getelementptr {ty}, {ty}* {source}, i32 0, i32 {index}"
    ));
    let byte = cg.emit_reg(format!("load i8, i8* {slot}"));
    cg.emit_reg(format!("icmp ne i8 {byte}, 0"))
}

pub(super) fn store_bool(cg: &mut Codegen, ty: &str, target: &str, index: usize, value: &str) {
    let byte = cg.emit_reg(format!("zext i1 {value} to i8"));
    let slot = cg.emit_reg(format!(
        "getelementptr {ty}, {ty}* {target}, i32 0, i32 {index}"
    ));
    cg.emit(format!("store i8 {byte}, i8* {slot}"));
}
