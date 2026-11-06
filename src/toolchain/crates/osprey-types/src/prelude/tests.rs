use super::*;
use osprey_ast::Position;
use osprey_syntax::parse_program;

fn program(source: &str) -> Program {
    let parsed = parse_program(source);
    assert!(parsed.errors.is_empty(), "syntax: {:?}", parsed.errors);
    parsed.program
}

fn attached_names(source: &str) -> Vec<String> {
    let source = program(source);
    with_prelude(&source)
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Type { name, .. } | Stmt::Function { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn statement_position(statement: &Stmt) -> Option<Position> {
    match statement {
        Stmt::Type { position, .. }
        | Stmt::Function { position, .. }
        | Stmt::Effect { position, .. } => *position,
        _ => None,
    }
}

fn statement_lines(unit: &Loaded, position_of: fn(&Stmt) -> Option<Position>) -> Vec<u32> {
    unit.program
        .statements
        .iter()
        .filter_map(position_of)
        .map(|position| position.line)
        .collect()
}

/// The unit's own source parses without syntax errors and rebases one-to-one.
#[track_caller]
fn assert_unit_rebases_cleanly(source: &str, unit: &Loaded) {
    let parsed = parse_program(source);
    assert!(parsed.errors.is_empty(), "syntax: {:?}", parsed.errors);
    assert_eq!(
        unit.program.statements.len(),
        parsed.program.statements.len()
    );
}

/// The lines are non-empty and every one satisfies `satisfies`.
#[track_caller]
fn assert_all_lines(lines: &[u32], satisfies: impl Fn(u32) -> bool) {
    assert!(
        !lines.is_empty() && lines.iter().all(|line| satisfies(*line)),
        "{lines:?}"
    );
}

/// Naming any `offered` declaration attaches the prelude exactly once per
/// name, and attaching the already-attached program again borrows it.
#[track_caller]
fn assert_offers_attach_once(sources: &[&str], offered: &str) {
    for source in sources {
        let names = attached_names(source);
        for name in offered.split_whitespace() {
            assert_eq!(
                names.iter().filter(|n| *n == name).count(),
                1,
                "{name} in {source}"
            );
        }
        let once = with_prelude(&program(source)).into_owned();
        assert!(
            matches!(with_prelude(&once), Cow::Borrowed(_)),
            "attaching twice: {source}"
        );
    }
}

/// Each source is left exactly as written: the prelude never attaches.
#[track_caller]
fn assert_left_as_written(sources: &[&str]) {
    for source in sources {
        let parsed = program(source);
        assert!(
            matches!(with_prelude(&parsed), Cow::Borrowed(_)),
            "must stay borrowed: {source}"
        );
    }
}

#[test]
fn the_prelude_source_parses_clean_and_lives_past_every_source_line() {
    let rebased = json();
    assert_unit_rebases_cleanly(JSON, rebased);
    assert!(
        attached(&rebased.program),
        "the prelude must recognise itself"
    );
    assert_all_lines(
        &statement_lines(rebased, statement_position),
        is_prelude_line,
    );
    assert!(!is_prelude_line(PRELUDE_LINE_BASE) && is_prelude_line(PRELUDE_LINE_BASE + 1));
}

#[test]
fn a_program_that_names_nothing_from_the_prelude_is_left_exactly_as_written() {
    // Naming an internal primitive does not pull the prelude in either.
    assert_left_as_written(&[
        "fn jsonish(x) = x\nprint(toString(jsonish(1)))\n",
        "print(toString(__jsonNodeKind(1, 0)))\n",
    ]);
}

#[test]
fn naming_any_offered_declaration_attaches_the_whole_prelude_once() {
    assert_offers_attach_once(
        &[
            "print(jsonEncode(JsonNull) ?: \"\")\n",
            "let decoded = jsonDecode(\"[]\")\n",
            "let member = JsonMember(\"k\", JsonInt(1))\n",
            "fn f(v) = jsonText(v)\n",
        ],
        "Json JsonMember jsonEncode jsonDecode jsonQuote jsonField jsonIndex jsonText jsonInt jsonFloat jsonBool",
    );
}

#[test]
fn a_program_declaring_a_prelude_type_or_constructor_keeps_its_own_and_gets_none() {
    assert_left_as_written(&[
        "type JsonMember = JsonMember(string, int)\nprint(jsonEncode(JsonNull) ?: \"\")\n",
        "type Json = { raw: string }\nlet j = Json { raw: \"x\" }\n",
        "type Mine = JsonNull | Other\nlet m = JsonNull\n",
    ]);
}

#[test]
fn a_program_function_replaces_only_the_prelude_function_it_names() {
    let names = attached_names("fn jsonField(a, b) = a + b\nprint(jsonEncode(JsonNull) ?: \"\")\n");
    assert_eq!(names.iter().filter(|name| *name == "jsonField").count(), 1);
    assert_eq!(
        names.last().map(String::as_str),
        Some("jsonField"),
        "the program's own copy stays"
    );
    assert!(
        names.iter().any(|name| name == "jsonQuote")
            && names.iter().any(|name| name == "__jsonMember")
    );
}

#[test]
fn the_prelude_type_checks_clean_beside_a_program_that_uses_all_of_it() {
    let source = program(
        "let doc = JsonObject([JsonMember(\"a\", JsonArray([JsonInt(1), JsonFloat(2.5), JsonBool(true), JsonNull]))])\n\
         let text = jsonEncode(doc) ?: \"\"\n\
         let back = jsonDecode(text) ?: JsonNull\n\
         let first = jsonField(back, \"a\") ?: JsonNull\n\
         let one = jsonIndex(first, 0) ?: JsonNull\n\
         print(\"${text} ${jsonQuote(\"q\") ?: \"\"} ${toString(jsonInt(one) ?: 0)} ${toString(jsonFloat(one) ?: 0.0)} ${toString(jsonBool(JsonBool(true)) ?: false)} ${jsonText(JsonString(\"s\")) ?: \"\"}\")\n",
    );
    assert_eq!(crate::check_program(&source), Vec::new());
}

#[test]
fn the_signals_source_parses_clean_and_lives_in_its_own_rebased_range() {
    let rebased = signals();
    assert_unit_rebases_cleanly(SIGNALS, rebased);
    let signals_base = PRELUDE_LINE_BASE + PRELUDE_UNIT_STRIDE;
    assert_all_lines(&statement_lines(rebased, statement_position), |line| {
        line >= signals_base
    });
    // The two units share no line: json's range ends where signals' begins.
    assert_all_lines(&statement_lines(json(), statement_position), |line| {
        line < signals_base
    });
    assert!(is_prelude_line(signals_base + 1));
}

#[test]
fn naming_a_signals_declaration_attaches_signals_and_json_exactly_once() {
    assert_offers_attach_once(
        &[
            "fn slice() = (perform Signal<Route>.read()).tab\n",
            "let frames = signalForced([], \"route\")\n",
            "let frame = SignalFrame(\"route\", [], 1)\n",
            "let keyed = signalRefreshKeyed([], \"route\", [], \"\", fn() => \"x\")\n",
            "let changed = signalChanges(1, 2, toString, [])\n",
            "let text = jsonEncode(signalFramesJson([])) ?: \"\"\n",
        ],
        "Json JsonMember jsonEncode jsonDecode SignalFrame signalFrameKey signalFrame signalRefreshKeyed signalChanges signalFramesJson signalFramesFromJson",
    );
}

#[test]
fn naming_only_a_json_declaration_attaches_json_but_not_signals() {
    let names = attached_names("print(jsonEncode(JsonNull) ?: \"\")\n");
    assert_eq!(names.iter().filter(|name| *name == "jsonEncode").count(), 1);
    assert!(
        !names.iter().any(|name| name == "SignalFrame"
            || name == "signalRefreshKeyed"
            || name == "signalChanges"),
        "signals must stay unattached: {names:?}"
    );
}

#[test]
fn a_program_declaring_its_own_signal_effect_keeps_it_and_gets_no_signals_unit() {
    let source = program(
        "static effect Signal<T> { read: fn() -> T }\n\
         type Route = { tab: string }\n\
         fn slice() = (perform Signal<Route>.read()).tab\n\
         print(jsonEncode(JsonNull) ?: \"\")\n",
    );
    let effects = with_prelude(&source)
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Effect { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(effects, ["Signal"], "only the program's own effect");
}

#[test]
fn the_signals_prelude_type_checks_beside_a_program_that_renders_keyed_regions() {
    let source = program(
        "type Route = { tab: string }\n\
         fn label() = \"route:${(perform Signal<Route>.read()).tab}\"\n\
         fn render(tab, frames, changed) = {\n\
             handle static Signal<Route> { read => Route { tab: tab } }\n\
             [signalFrame(signalRefreshKeyed(frames, \"route\", changed, \"\", label))]\n\
         }\n\
         let first = render(\"home\", [], [\"<unknown>\"])\n\
         let away = Route { tab: \"away\" }\n\
         let next = render(\"away\", first, signalChanges(Route { tab: \"home\" }, away, fn(route) => JsonString(route.tab), signalDependencies(label)))\n\
         print(\"${toString(signalRevisionOf(first, \"route\"))}/${toString(signalRevisionOf(next, \"route\"))}\")\n",
    );
    assert_eq!(crate::check_program(&source), Vec::new());
}

#[test]
fn the_testing_source_parses_clean_and_lives_past_json_and_signals() {
    let rebased = testing();
    assert_unit_rebases_cleanly(TESTING, rebased);
    let testing_base = PRELUDE_LINE_BASE + 2 * PRELUDE_UNIT_STRIDE;
    assert_all_lines(&statement_lines(rebased, statement_position), |line| {
        line >= testing_base
    });
    assert_all_lines(&statement_lines(signals(), statement_position), |line| {
        line < testing_base
    });
    assert!(
        attached(&rebased.program),
        "the testing unit must recognise itself"
    );
}

#[test]
fn naming_the_testing_policy_attaches_it_once_and_no_json_or_signals() {
    let sources = [
        "test(\"sum\", fn() => exactArithmetic(fn() => expect(1 + 2, 3)))\n",
        "fn case() = check(\"q\", \"2\", toString(7 / 3))\ntest(\"q\", fn() => exactArithmetic(case))\n",
        "let policy = exactArithmetic\npolicy(fn() => print(toString(9 * 9)))\n",
        "exactArithmetic fn () => print (toString (2 + 2))\n",
    ];
    assert_offers_attach_once(&sources[..3], "exactArithmetic");
    for source in &sources[..3] {
        let names = attached_names(source);
        assert!(
            !names
                .iter()
                .any(|name| name == "Json" || name == "SignalFrame"),
            "only the testing unit attaches: {names:?}"
        );
    }
}

#[test]
fn a_program_declaring_its_own_testing_policy_keeps_its_own() {
    let names = attached_names(
        "fn exactArithmetic(work) = work()\ntest(\"own\", fn() => exactArithmetic(fn() => expect(1, 1)))\n",
    );
    assert_eq!(
        names.iter().filter(|name| *name == "exactArithmetic").count(),
        1,
        "only the program's own copy: {names:?}"
    );
    assert_eq!(names.last().map(String::as_str), Some("exactArithmetic"));
    assert_left_as_written(&["test(\"plain\", fn() => expect(1, 1))\n"]);
}

#[test]
fn the_testing_policy_discharges_every_arith_fault_in_the_work_it_runs() {
    for body in [
        "expect(9223372036854775807 + 0, 9223372036854775807)",
        "check(\"div\", \"2.5\", toString(5 / 2))",
        "expect(rest(3), 1)",
        "expect(abs(0 - 5) * 2, 10)",
    ] {
        let source = program(&format!(
            "fn rest(n) = 7 % n\ntest(\"faults\", fn() => exactArithmetic(fn() => {body}))\n"
        ));
        assert_eq!(crate::check_program(&source), Vec::new(), "{body}");
    }
    let unhandled = program("fn rest(n) = 7 % n\ntest(\"bare\", fn() => expect(rest(3), 1))\n");
    assert!(
        !crate::check_program(&unhandled).is_empty(),
        "without the policy the program is rejected"
    );
}

#[test]
fn the_gpu_result_helpers_attach_with_the_testing_unit_and_type_check() {
    assert_offers_attach_once(
        &[
            "test(\"w\", fn() => withBuffer(toGpu([1, 2]), fn(b) => expect(gpuLength(b), 2)))\n",
            "test(\"s\", fn() => checkTrue(gpuSatisfies(toGpu([3]), fn(b) => gpuLength(b) == 1)))\n",
            "test(\"i\", fn() => withIndices(4, fn(b) => expect(gpuLength(b), 4)))\n",
            "print(iotaError(0 - 1))\n",
        ],
        "withBuffer gpuSatisfies withIndices iotaError exactArithmetic",
    );
    let source = program(
        "test(\"all\", fn() => exactArithmetic(fn() => withIndices(3, fn(b) => checkAll(\"gpu\", [gpuSatisfies(gpuMap(b, fn(x) => x * 2), fn(d) => gpuLength(d) == 3), iotaError(0 - 1) != \"\"]))))\n",
    );
    assert_eq!(crate::check_program(&source), Vec::new());
}

#[test]
fn the_text_assertion_helpers_attach_and_type_check() {
    assert_offers_attach_once(
        &[
            "test(\"a\", fn() => checkTrue(containsAll(\"alpha|beta\", [\"alpha\", \"beta\"])))\n",
            "test(\"n\", fn() => checkTrue(containsNone(\"alpha|beta\", [\"gamma\"])))\n",
        ],
        "containsAll containsNone exactArithmetic",
    );
    let source = program(
        "test(\"text\", fn() => checkAll(\"parts\", [containsAll(\"12|spare|9\", [\"12|\", \"|spare|\", \"|9\"]), containsNone(\"12|spare|9\", [\"2.5\", \"|13|\"]), !containsAll(\"ab\", [\"a\", \"c\"]), !containsNone(\"ab\", [\"b\"]), containsAll(\"\", []), containsNone(\"\", [])]))\n",
    );
    assert_eq!(crate::check_program(&source), Vec::new());
}
