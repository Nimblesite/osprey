//! [BOOL-EQUALITY] and [BOOL-ORDERING]: which comparisons are decided.
use crate::testutil::{bad_with, ok};

const UNDECIDED: &str = "equality is decided only for int, float, string, bool";
const UNORDERED: &str = "ordering is decided only for int, float, string and bool";

#[test]
fn scalars_compare_with_every_operator() {
    ok(
        "let a = 1 == 2\nlet b = 1.5 != 2.5\nlet c = \"x\" == \"y\"\nlet d = true == false\n\
        let e = 1 < 2\nlet f = 1.5 >= 2.5\nlet g = \"a\" <= \"b\"\nlet h = false < true\n",
    );
}

#[test]
fn payload_free_union_values_compare_by_variant() {
    ok("type Area = Apps | Files\n\
        fn same(a, b) = a == b\n\
        let x = same(Apps, Files)\n\
        let y = Apps != Files\n");
}

#[test]
fn a_payload_free_constructor_is_a_variant_test_on_any_union() {
    ok("type Shape = Dot | Circle { r: int }\n\
        fn isDot(shape) = shape == Dot\n\
        let x = isDot(Circle { r: 1 })\n\
        let y = Circle { r: 2 } != Dot\n");
}

#[test]
fn a_record_has_no_decided_equality() {
    bad_with(
        "type Route = { tab: string }\n\
         let x = Route { tab: \"a\" } == Route { tab: \"a\" }\n",
        UNDECIDED,
    );
}

#[test]
fn a_list_has_no_decided_equality() {
    bad_with("let x = [1] == [1]\n", UNDECIDED);
}

#[test]
fn union_values_that_may_carry_payloads_have_no_decided_equality() {
    bad_with(
        "type Shape = Dot | Circle { r: int }\n\
         fn same(a, b) = a == b\n\
         let x = same(Circle { r: 2 }, Circle { r: 2 })\n",
        UNDECIDED,
    );
}

#[test]
fn a_function_has_no_decided_equality() {
    bad_with(
        "fn f(x) = x + 1\nfn g(x) = x + 2\nlet x = f == g\n",
        UNDECIDED,
    );
}

#[test]
fn a_generic_comparison_is_checked_at_each_call_site() {
    ok("fn same(a, b) = a == b\nlet x = same(1, 2)\nlet y = same(\"a\", \"b\")\n");
    bad_with(
        "fn same(a, b) = a == b\nlet x = same([1], [1])\n",
        UNDECIDED,
    );
}

#[test]
fn a_payload_free_union_has_no_decided_order() {
    bad_with(
        "type Area = Apps | Files\nfn before(a, b) = a < b\nlet x = before(Apps, Files)\n",
        UNORDERED,
    );
}

#[test]
fn a_record_has_no_decided_order() {
    bad_with(
        "type Route = { tab: string }\n\
         let x = Route { tab: \"a\" } < Route { tab: \"b\" }\n",
        UNORDERED,
    );
}
