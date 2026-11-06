//! [BUILTIN-STRUCTURAL-EQUALS]: which types `structuralEquals` may walk. The
//! accepted types come from the spec table (scalars, records, unions, lists,
//! maps); the rejected ones are the parts whose equality is not decided.
use crate::testutil::{bad_with, ok};

const FN_FIELD: &str = "field `onClick` is a function, whose equality is not decided";
const FN_ELEMENT: &str = "list element is a function, whose equality is not decided";
const FN_VALUE: &str = "map value is a function, whose equality is not decided";
const HANDLE: &str = "is a runtime handle, whose equality is not decided";
const ANY: &str = "is `any`, whose equality is not decided";
const RESULT: &str = "is a `Result`, whose equality is not decided";

#[test]
fn scalars_have_decided_equality() {
    ok("let a = structuralEquals(1, 2)\n\
        let b = structuralEquals(\"x\", \"y\")\n\
        let c = structuralEquals(true, false)\n\
        let d = structuralEquals(0.5, 0.25)\n\
        let e = structuralEquals(sqrt(-1.0), sqrt(-1.0))\n");
}

#[test]
fn records_and_nested_records_have_decided_equality() {
    ok("type Route = { tab: string, depth: int }\n\
        type Box = { inner: Route, live: bool }\n\
        let a = structuralEquals(Route { tab: \"home\", depth: 1 }, Route { tab: \"home\", depth: 1 })\n\
        let b = structuralEquals(Box { inner: Route { tab: \"a\", depth: 2 }, live: true },\
                                 Box { inner: Route { tab: \"a\", depth: 2 }, live: true })\n");
}

#[test]
fn unions_with_payloads_have_decided_equality() {
    ok(
        "type Shape = Dot | Circle { r: float } | Rect { w: int, h: int }\n\
        let a = structuralEquals(Circle { r: 1.0 }, Circle { r: 1.0 })\n\
        let b = structuralEquals(Dot, Rect { w: 1, h: 2 })\n",
    );
}

#[test]
fn lists_and_maps_have_decided_equality() {
    ok("type Route = { tab: string }\n\
        let a = structuralEquals([1, 2], [1, 2])\n\
        let b = structuralEquals([[1], [2]], [[1], [2]])\n\
        let c = structuralEquals(Map() |> mapSet(\"a\", 1), Map() |> mapSet(\"a\", 1))\n\
        let d = structuralEquals([Route { tab: \"a\" }], [Route { tab: \"a\" }])\n");
}

#[test]
fn a_recursive_tree_terminates() {
    ok(
        "type Tree = Leaf | Node { left: Tree, right: Tree, size: int }\n\
        let a = Node { left: Leaf, right: Node { left: Leaf, right: Leaf, size: 1 }, size: 2 }\n\
        let b = Node { left: Leaf, right: Node { left: Leaf, right: Leaf, size: 1 }, size: 2 }\n\
        let same = structuralEquals(a, b)\n",
    );
}

#[test]
fn a_generic_caller_is_checked_at_each_call_site() {
    ok("fn same(a, b) = structuralEquals(a, b)\n\
        let x = same(1, 2)\n\
        let y = same([\"a\"], [\"a\"])\n");
    bad_with(
        "type Widget = { onClick: fn(int) -> int }\n\
         fn same(a, b) = structuralEquals(a, b)\n\
         let w = Widget { onClick: fn(n) => n }\n\
         let x = same(w, w)\n",
        FN_FIELD,
    );
}

#[test]
fn a_function_field_is_rejected() {
    bad_with(
        "type Widget = { tab: string, onClick: fn(int) -> int }\n\
         let w = Widget { tab: \"home\", onClick: fn(n) => n }\n\
         let x = structuralEquals(w, w)\n",
        FN_FIELD,
    );
}

#[test]
fn a_list_of_functions_is_rejected() {
    bad_with(
        "let f = fn(n) => n\nlet x = structuralEquals([f], [f])\n",
        FN_ELEMENT,
    );
}

#[test]
fn a_map_of_functions_is_rejected() {
    bad_with(
        "let f = fn(n) => n\nlet m = Map() |> mapSet(\"go\", f)\n\
         let x = structuralEquals(m, m)\n",
        FN_VALUE,
    );
}

#[test]
fn a_fiber_is_rejected() {
    bad_with(
        "fn work(n) = n + 1\n\
         let a = spawn work(1)\n\
         let b = spawn work(1)\n\
         let x = structuralEquals(a, b)\n",
        HANDLE,
    );
}

#[test]
fn a_channel_is_rejected() {
    bad_with(
        "let a = Channel(1)\nlet b = Channel(1)\nlet x = structuralEquals(a, b)\n",
        HANDLE,
    );
}

#[test]
fn any_is_rejected() {
    bad_with(
        "let a: any = 1\nlet b: any = 2\nlet x = structuralEquals(a, b)\n",
        ANY,
    );
}

#[test]
fn a_result_is_rejected() {
    bad_with(
        "let a = Success { value: 1 }\nlet b = Success { value: 2 }\n\
         let x = structuralEquals(a, b)\n",
        RESULT,
    );
}

#[test]
fn the_diagnostic_names_the_compared_type() {
    bad_with(
        "type Widget = { onClick: fn(int) -> int }\n\
         let w = Widget { onClick: fn(n) => n }\n\
         let x = structuralEquals(w, w)\n",
        "cannot compare `{ onClick: (int) -> int }` with `structuralEquals`: field `onClick` is a function",
    );
}
