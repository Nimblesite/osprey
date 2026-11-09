//! Match lowering tests.

use super::*;
use crate::llty::LType;

#[test]
fn nested_list_join_keeps_concrete_inference_beside_an_untyped_empty_default() {
    let mut cg = Codegen::new();
    let mut row = Value::handle("%row", "List#List#i64");
    let ints = osprey_types::Type::con("List", vec![osprey_types::Type::con("int", vec![])]);
    row.inferred_type = Some(osprey_types::Type::con("List", vec![ints.clone()]));
    let fallback = Value::handle("%empty", "List#List#i8*");
    let joined = finish_phi(
        &mut cg,
        &[(row, "row".into()), (fallback, "empty".into())],
        "end",
        0,
    )
    .expect("both branches carry list pointers");
    let element = crate::collections::elem_value(&mut cg, &joined, "%word");
    assert_eq!(element.ty, LType::Ptr);
    assert_eq!(element.osp_ty.as_deref(), Some("List#i64"));
    assert_eq!(element.inferred_type, Some(ints));
}
