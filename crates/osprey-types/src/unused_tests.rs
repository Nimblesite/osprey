//! Scope and source-identity assertions for [TYPE-WARNINGS-UNUSED].

use crate::{unused_symbols, UnusedKind, UnusedSymbol};
use osprey_ast::Position;
use osprey_syntax::{parse_program_with_flavor, Flavor};

fn warnings(flavor: Flavor, source: &str) -> Vec<UnusedSymbol> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
    let errors = crate::check_program(&parsed.program);
    assert!(errors.is_empty(), "{source}\n{errors:?}");
    let warnings = unused_symbols(&parsed.program);
    let ranges = osprey_syntax::binding_ranges(source, flavor);
    for warning in &warnings {
        let found: Vec<_> = ranges
            .iter()
            .filter(|range| {
                range.owner_position == warning.owner_position
                    && range.name == warning.name
                    && range.occurrence == warning.occurrence
                    && range.kind == binding_kind(warning.kind)
            })
            .collect();
        assert_eq!(
            found.len(),
            1,
            "unmapped or ambiguous {warning:?}\n{ranges:?}\n{source}"
        );
        assert_eq!(
            found
                .first()
                .and_then(|range| source.get(range.range.clone())),
            Some(warning.name.as_str())
        );
    }
    warnings
}

fn binding_kind(kind: UnusedKind) -> osprey_syntax::BindingKind {
    use osprey_syntax::BindingKind;
    match kind {
        UnusedKind::Variable => BindingKind::Variable,
        UnusedKind::Parameter => BindingKind::Parameter,
        UnusedKind::PatternBinding => BindingKind::PatternBinding,
        UnusedKind::HandlerParameter => BindingKind::HandlerParameter,
    }
}

fn reports(flavor: Flavor, source: &str, expected: &[(&str, &str)]) {
    let actual = warnings(flavor, source);
    let rendered: Vec<_> = actual
        .iter()
        .map(|s| (s.warning.rule, s.warning.message.as_str()))
        .collect();
    assert_eq!(rendered, expected, "{source}\n{actual:?}");
    assert!(actual
        .iter()
        .all(|s| s.warning.position == s.owner_position));
}

#[test]
fn parameters_and_locals_are_named_in_declaration_order() {
    let source = "fn choose(used, ignored) = { let spare = 1\nused }\nlet result = choose(2, 3)\n";
    reports(
        Flavor::Default,
        source,
        &[
            ("unused-parameter", "unused parameter `ignored`"),
            ("unused-variable", "unused variable `spare`"),
        ],
    );
    let actual = warnings(Flavor::Default, source);
    assert_eq!(
        actual
            .first()
            .map(|s| (&s.name, s.kind, s.owner_position, s.occurrence)),
        Some((
            &"ignored".to_string(),
            UnusedKind::Parameter,
            Some(Position { line: 1, column: 3 }),
            0
        ))
    );
}

#[test]
fn shadows_do_not_count_as_reads_of_the_outer_binding() {
    reports(
        Flavor::Default,
        "fn shadow(value) = { let value = 7\nvalue }\nlet result = shadow(1)\n",
        &[("unused-parameter", "unused parameter `value`")],
    );
    reports(
        Flavor::Default,
        "fn shadow(value) = { let value = value\nvalue }\nlet result = shadow(1)\n",
        &[],
    );
}

#[test]
fn closure_capture_is_a_read_but_shadowing_lambda_parameter_is_not() {
    reports(
        Flavor::Default,
        "fn outer(value) = fn(other) => value\nlet result = outer(1)(2)\n",
        &[("unused-parameter", "unused parameter `other`")],
    );
    reports(
        Flavor::Default,
        "fn outer(value) = fn(value) => value\nlet result = outer(1)(2)\n",
        &[("unused-parameter", "unused parameter `value`")],
    );
}

#[test]
fn intentional_names_top_level_bindings_and_extern_contracts_are_silent() {
    reports(Flavor::Default,
        "extern fn native(input: int) -> int\nfn public(_unused, _) = { let _spare = 1\n2 }\nlet externallyVisible = 3\n", &[]);
    reports(Flavor::Ml,
        "keep _unused _ =\n    _spare = 1\n    2\nextern native (input : int) -> int\nexternallyVisible = 3\n", &[]);
}

#[test]
fn assignments_do_not_make_an_unread_variable_used() {
    reports(Flavor::Default,
        "effect Set { put: fn(int) -> Unit }\nfn work() = { mut scratch = 1\nhandle Set\n put value => { scratch = value }\nin { perform Set.put(2) }\n3 }\nlet result = work()\n",
        &[("unused-variable", "unused variable `scratch`")]);
    reports(Flavor::Default,
        "effect Set { put: fn(int) -> Unit }\nfn work() = { mut scratch = 1\nhandle Set\n put value => { scratch = value }\nin { perform Set.put(2) }\nscratch }\nlet result = work()\n", &[]);
}

#[test]
fn nested_pattern_bindings_have_independent_arm_scopes() {
    reports(Flavor::Default,
        "type Pair = { left: int, right: int }\nfn first(pair: Pair) = match pair { { left, right } => left }\nlet result = first(Pair { left: 1, right: 2 })\n",
        &[("unused-pattern-binding", "unused pattern binding `right`")]);
    reports(Flavor::Default,
        "fn first(items: List<int>) = match items { [head, ...tail] => head\n[] => 0 }\nlet result = first([1, 2])\n",
        &[("unused-pattern-binding", "unused pattern binding `tail`")]);
}

#[test]
fn handlers_and_fibers_preserve_outer_captures() {
    reports(Flavor::Default,
        "effect Pick { choose: fn(int, int) -> int }\nfn work(seed) = handle Pick\n choose first second => resume(first)\nin await (spawn (perform Pick.choose(seed, 2)))\nlet result = work(7)\n",
        &[("unused-handler-parameter", "unused handler parameter `second` of `Pick.choose`")]);
}

#[test]
fn curried_ml_parameters_and_locals_follow_the_lowered_scopes() {
    reports(
        Flavor::Ml,
        "choose used ignored =\n    spare = 1\n    used\nresult = choose 2 3\n",
        &[
            ("unused-parameter", "unused parameter `ignored`"),
            ("unused-variable", "unused variable `spare`"),
        ],
    );
}

#[test]
fn invalid_programs_never_raise_speculative_unused_warnings() {
    let parsed = parse_program_with_flavor("fn broken(unused) = missing\n", Flavor::Default);
    assert!(parsed.errors.is_empty());
    assert!(!crate::check_program(&parsed.program).is_empty());
    assert!(unused_symbols(&parsed.program).is_empty());
}

#[test]
fn signed_inline_constraints_retain_the_source_parameter_identity() {
    reports(Flavor::Ml,
        "choose : int -> int -> int\nchoose (unused : int) (used : int) = used\nresult = choose 1 2\n",
        &[("unused-parameter", "unused parameter `unused`")]);
    reports(Flavor::Ml,
        "choose : int -> int -> int\nchoose (used : int) (unused : int) = used\nresult = choose 1 2\n",
        &[("unused-parameter", "unused parameter `unused`")]);
    reports(Flavor::Ml,
        "choose : (int, int) -> int\nchoose (unused : int, used : int) = used\nresult = choose (1, 2)\n",
        &[("unused-parameter", "unused parameter `unused`")]);
}

#[test]
fn inline_constraint_provenance_is_explicit_and_not_inferred_from_source_shape() {
    for (flavor, source, expected) in [
        (
            Flavor::Ml,
            "choose : int -> int\nchoose (value : int) = value\n",
            true,
        ),
        (Flavor::Ml, "choose (value : int) = value\n", false),
        (
            Flavor::Default,
            "fn choose(value: int) = {\n let value: int = value\n value\n}\n",
            false,
        ),
    ] {
        let parsed = parse_program_with_flavor(source, flavor);
        assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
        let Some(osprey_ast::Stmt::Function { parameters, .. }) = parsed.program.statements.first()
        else {
            panic!("function missing: {source}");
        };
        assert_eq!(
            parameters
                .first()
                .map(|parameter| parameter.inline_constraint),
            Some(expected),
            "{source}"
        );
    }
}

#[test]
fn a_written_self_copy_is_a_local_even_when_its_shape_matches_lowering() {
    reports(
        Flavor::Default,
        "fn choose(value: int) = {\n let value: int = value\n 1\n}\nlet result = choose(2)\n",
        &[("unused-variable", "unused variable `value`")],
    );
    reports(Flavor::Ml,
        "choose : int -> int\nchoose (value : int) =\n    value : int\n    value = value\n    1\nresult = choose 2\n",
        &[("unused-variable", "unused variable `value`")]);
}

#[test]
fn constructors_are_not_pattern_binders() {
    reports(Flavor::Default,
        "type Choice = Empty | Full(int)\nfn choose(value: Choice) = match value {\n Empty => 0\n Full(unused) => 1\n}\nlet result = choose(Empty)\n",
        &[("unused-pattern-binding", "unused pattern binding `unused`")]);
}

#[test]
fn repeated_pattern_names_are_resolved_in_their_own_arms() {
    let source = "type Choice = Left(int) | Right(int)\nfn choose(value: Choice) = match value {\n Left(item) => item\n Right(item) => 0\n}\nlet result = choose(Left(1))\n";
    reports(
        Flavor::Default,
        source,
        &[("unused-pattern-binding", "unused pattern binding `item`")],
    );
    let actual = warnings(Flavor::Default, source);
    assert_eq!(actual.first().map(|s| s.occurrence), Some(1));
}

#[test]
fn callable_field_does_not_read_a_same_named_local_but_ufcs_does() {
    reports(Flavor::Default,
        "fn choose() = {\n let run = fn(value) => value\n let box = { run: fn(value) => value }\n box.run(1)\n}\nlet result = choose()\n",
        &[("unused-variable", "unused variable `run`")]);
    reports(
        Flavor::Default,
        "fn choose(run) = 1.run()\nlet result = choose(fn(value) => value)\n",
        &[],
    );
}

#[test]
fn collection_values_named_arguments_and_interpolation_read_locals() {
    reports(Flavor::Default,
        "fn choose(value) = {\n let values = [value]\n let lookup = { \"a\": value }\n let callback = fn(first, second) => \"${first} ${second}\"\n callback(second: lookup, first: values)\n}\nlet result = choose(2)\n", &[]);
}

#[test]
fn recursive_calls_and_covariant_generic_records_preserve_reads() {
    reports(Flavor::Default,
        "type Box<out T> = { value: T }\nfn descend<T>(value: Box<T>, again) = match again {\n true => descend<T>(value, false)\n false => value.value\n}\nlet result = descend<int>(Box<int> { value: 2 }, true)\n", &[]);
}

#[test]
fn record_update_reads_its_base_and_replacement() {
    reports(Flavor::Default,
        "type Box = { value: int }\nfn choose(value) = {\n let box = Box { value: 1 }\n let changed = box { value: value }\n changed.value\n}\nlet result = choose(2)\n", &[]);
}

#[test]
fn binders_are_source_ordered_before_their_initializer_lambdas() {
    reports(
        Flavor::Default,
        "fn choose() = {\n let unused = fn(ignored) => 1\n 2\n}\nlet result = choose()\n",
        &[
            ("unused-variable", "unused variable `unused`"),
            ("unused-parameter", "unused parameter `ignored`"),
        ],
    );
}

#[test]
fn module_exports_and_namespace_members_are_not_mislabelled_unused() {
    reports(Flavor::Default,
        "module Public {\n export fn choose(used, ignored) = used\n}\nnamespace other { fn identity(value) = value }\n",
        &[("unused-parameter", "unused parameter `ignored`")]);
}

#[test]
fn interpolation_lambdas_have_real_ranges_even_after_escapes_and_unicode() {
    reports(
        Flavor::Default,
        "let text = \"🦅\\n${(fn(value: int, ignored) => value)(1, 2)}\"\n",
        &[("unused-parameter", "unused parameter `ignored`")],
    );
    reports(
        Flavor::Ml,
        "text = \"🦅\\n${(\\(value : int, ignored) => value) (1, 2)}\"\n",
        &[("unused-parameter", "unused parameter `ignored`")],
    );
}

#[test]
fn top_level_matches_keep_distinct_source_owners_for_repeated_binders() {
    for (flavor, source, lines) in [
        (
            Flavor::Default,
            "match 1 { unused => print(\"a\") }\nmatch 2 { unused => print(\"b\") }\n",
            [1, 2],
        ),
        (
            Flavor::Ml,
            "match 1\n    unused => print \"a\"\nmatch 2\n    unused => print \"b\"\n",
            [1, 3],
        ),
    ] {
        let actual = warnings(flavor, source);
        assert_eq!(actual.len(), 2, "{actual:?}");
        for (warning, line) in actual.iter().zip(lines) {
            assert_eq!(warning.warning.rule, "unused-pattern-binding");
            assert_eq!(warning.warning.message, "unused pattern binding `unused`");
            assert_eq!(warning.owner_position, Some(Position { line, column: 0 }));
            assert_eq!(warning.occurrence, 0);
        }
    }
}

#[test]
fn top_level_interpolation_patterns_inherit_the_source_expression_owner() {
    for (flavor, source) in [
        (
            Flavor::Default,
            "print(\"🦅\\n${match 1 { unused => 2 }}\")\n",
        ),
        (Flavor::Ml, "print \"🦅\\n${match 1\\n    unused => 2}\"\n"),
    ] {
        let actual = warnings(flavor, source);
        assert_eq!(actual.len(), 1, "{source}\n{actual:?}");
        let warning = actual.first().expect("one pattern warning");
        assert_eq!(warning.name, "unused");
        assert_eq!(
            warning.owner_position,
            Some(Position { line: 1, column: 0 })
        );
        assert_eq!(warning.occurrence, 0);
    }
}

#[test]
fn nested_expression_statements_keep_their_enclosing_function_owner() {
    for (flavor, source, column) in [
        (
            Flavor::Default,
            "fn main() = {\n match 1 { unused => print(\"a\") }\n print(\"b\")\n}\n",
            3,
        ),
        (
            Flavor::Ml,
            "main () =\n    match 1\n        unused => print \"a\"\n    print \"b\"\n",
            0,
        ),
    ] {
        let actual = warnings(flavor, source);
        assert_eq!(actual.len(), 1, "{actual:?}");
        assert_eq!(
            actual.first().expect("one warning").owner_position,
            Some(Position { line: 1, column })
        );
    }
}
