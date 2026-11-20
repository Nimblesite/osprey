//! Inferred signatures on hover: resolved slots, holes and built-in
//! constraints [LSP-HOVER-INFERRED-SIGNATURE] [TYPE-CONSTRAINTS].

use crate::test_support::hover;
use crate::testkit::shows;
use lspkit::vfs::PositionEncoding;
const U16: PositionEncoding = PositionEncoding::Utf16;

#[test]
fn an_inferred_signature_fills_resolved_slots_and_holes_the_unsolved_ones() {
    // Implements [LSP-HOVER-INFERRED-SIGNATURE]. `r` resolves to a concrete
    // `int`, so the reader gets it.
    let concrete = hover(
        "fn area(r) = r * r\nlet a = area(5)\n",
        "file:///a.osp",
        0,
        3,
        U16,
    )
    .expect("hover over `area`");
    shows(&concrete, &["fn area(r: int)"]);
    // `xs` is proven to be a LIST; only its element type stays open. The
    // choice used to be `List<t5>` or nothing, and nothing won, because
    // `t5` is an inference artefact whose number shifts when an unrelated
    // line moves. A hole is neither: it keeps everything the checker proved
    // and names nothing it did not ([`osprey_types::render_with_holes`]).
    let generic = hover(
            "fn classify(xs) = match xs {\n  [] => 0\n  [head, ...tail] => 1\n}\nlet e = classify([])\n",
            "file:///b.osp",
            0,
            3,
            U16,
        )
        .expect("hover over `classify`");
    shows(&generic, &["fn classify(xs: List<_>) -> int"]);
    // The artefact itself stays banned, which is what the bare rendering
    // was really protecting.
    assert!(
        !generic.contains("t5") && !generic.contains("<t"),
        "an inference artefact must never reach a reader: {generic}"
    );
    // [TYPE-CONSTRAINTS]: a variable a built-in constrains is shown as its
    // constraint, through every generic level, and never as a bare hole.
    let constrained = "fn show(x) = toString(x)\nfn twice(x) = show(x)\nfn size(xs) = length(xs)\nfn sorted(xs) = listSort(xs)\nfn same(a, b) = a < b\n";
    for (line, expected) in [
        (0, "fn show(x: printable) -> string"),
        (1, "fn twice(x: printable) -> string"),
        (2, "fn size(xs: sized) -> int"),
        (3, "fn sorted(xs: List<sortable>) -> List<sortable>"),
        (4, "fn same(a: comparable, b: comparable) -> bool"),
    ] {
        let md = hover(constrained, "file:///c.osp", line, 3, U16).expect("hover a constrained fn");

        shows(&md, &[expected]);
    }
    let ml = hover(
        "show x = toString x\ntwice x = show x\n",
        "file:///c.ospml",
        1,
        0,
        U16,
    )
    .expect("hover an ML constrained fn");
    crate::testkit::contents(&ml, &["twice : printable -> string"], &["_"]);
}
