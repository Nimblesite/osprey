//! The replacement handler grammar and declaration-bound continuation scope.
//! Implements [EFFECTS-HANDLE-REST], [EFFECTS-HANDLER-VALUE], [EFFECTS-RESUME].

use osprey_syntax::{parse_program_with_flavor, Flavor};

#[test]
fn removed_handler_applications_are_syntax_errors() {
    for source in [
        "effect E { op: fn() -> int }\nlet x = handle E op => 1 in perform E.op()",
        "effect E { op: fn() -> int }\nlet x = handle E op => 1 do perform E.op()",
        "effect E { op: fn() -> int }\nlet x = handle E { op => 1 }",
        "effect E { op: fn() -> int }\nfn f() = { handle E { op => 1 } in perform E.op() }",
        "effect E { op: fn() -> int }\nhandle E { op => 1 }\nprint(1)",
        "effect E { op: fn() -> int }\nfn f() = { handle E { op => 1 } // no computation\n}",
    ] {
        let parsed = parse_program_with_flavor(source, Flavor::Default);
        assert!(!parsed.errors.is_empty(), "removed form parsed: {source}");
    }
    for source in [
        "effect E\n    op : Unit => int\nx = handle E\n    op => 1\nin perform E.op ()\n",
        "effect E\n    op : Unit => int\nf () =\n    handle E\n        op => 1\n    in perform E.op ()\n",
    ] {
        let parsed = parse_program_with_flavor(source, Flavor::Ml);
        assert!(!parsed.errors.is_empty(), "removed form parsed: {source}");
    }
}

#[test]
fn value_arms_have_no_continuation_even_inside_a_control_arm() {
    let default = r"
effect Value { op: fn() -> int }
effect Control { control op: fn() -> int }
let h = handler Control {
    op => {
        let nested = handler Value { op => resume(1) }
        nested(|| => perform Value.op())
    }
}
print(h(|| => perform Control.op()))
";
    let ml = r"
effect Value
    op : Unit => int
effect Control
    control op : Unit => int
h = handler Control
    op =>
        nested = handler Value
            op => resume 1
        nested (\() => perform Value.op ())
print (h (\() => perform Control.op ()))
";
    for (source, flavor) in [(default, Flavor::Default), (ml, Flavor::Ml)] {
        let parsed = parse_program_with_flavor(source, flavor);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let errors = osprey_types::check_program(&parsed.program);
        assert!(
            errors.iter().any(|error| error
                .message
                .contains("continuation of a control operation")),
            "{errors:?}"
        );
    }
}

#[test]
fn block_scoped_and_callable_handlers_have_one_contract() {
    let default = r#"
effect E { op: fn() -> int }
fn block() = {
    handle E { op => 42 }
    perform E.op()
}
let h = handler E { op => 42 }
let a = block()
let b = h(|| => perform E.op())
print("${a}:${b}")
"#;
    let ml = r#"
effect E
    op : Unit => int
block () =
    handle E
        op => 42
    perform E.op ()
h = handler E
    op => 42
a = block ()
b = h (\() => perform E.op ())
print "${a}:${b}"
"#;
    for (source, flavor) in [(default, Flavor::Default), (ml, Flavor::Ml)] {
        let parsed = parse_program_with_flavor(source, flavor);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let errors = osprey_types::check_program(&parsed.program);
        assert!(errors.is_empty(), "{errors:?}");
        let compiled = osprey_codegen::compile_program(&parsed.program);
        assert!(compiled.is_ok(), "{compiled:?}");
    }
}
