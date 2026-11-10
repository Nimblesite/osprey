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

/// How many declarations called `name` the program carries.
fn count_named(names: &[String], name: &str) -> usize {
    names.iter().filter(|n| *n == name).count()
}

/// The lines are non-empty and every one satisfies `satisfies`.
#[track_caller]
fn assert_all_lines(lines: &[u32], satisfies: impl Fn(u32) -> bool) {
    assert!(
        !lines.is_empty() && lines.iter().all(|line| satisfies(*line)),
        "{lines:?}"
    );
}

/// The unit's own source parses without syntax errors, rebases one-to-one,
/// and its statements satisfy `satisfies` from `base` on while every unit in
/// `earlier` keeps its statements strictly below `base`.
#[track_caller]
fn assert_in_range(
    source: &str,
    unit: &Loaded,
    base: u32,
    satisfies: impl Fn(u32) -> bool,
    earlier: &[&Loaded],
) {
    let parsed = parse_program(source);
    assert!(parsed.errors.is_empty(), "syntax: {:?}", parsed.errors);
    assert_eq!(
        unit.program.statements.len(),
        parsed.program.statements.len()
    );
    assert_all_lines(&statement_lines(unit, statement_position), satisfies);
    for prior in earlier {
        assert_all_lines(&statement_lines(prior, statement_position), |line| {
            line < base
        });
    }
}

/// The program type checks with no diagnostics, reported under `context`.
#[track_caller]
fn assert_type_checks(source: &str, context: &str) {
    assert_eq!(
        crate::check_program(&program(source)),
        Vec::new(),
        "{context}"
    );
}

/// None of `unwanted` appears among the attached names.
#[track_caller]
fn assert_none_attached(names: &[String], unwanted: &[&str]) {
    for name in unwanted {
        assert!(!names.contains(&name.to_string()), "{name}: {names:?}");
    }
}

/// Shadowing a prelude name leaves exactly one declaration of `name`, and
/// attaching puts the program's own copy last so it wins.
#[track_caller]
fn assert_program_shadow_wins(names: &[String], name: &str) {
    assert_eq!(count_named(names, name), 1, "{names:?}");
    assert_eq!(
        names.last().map(String::as_str),
        Some(name),
        "the program's own copy stays: {names:?}"
    );
}

/// Naming any `offered` declaration attaches the prelude exactly once per
/// name, and attaching the already-attached program again borrows it.
#[track_caller]
fn assert_offers_attach_once(sources: &[&str], offered: &str) {
    for source in sources {
        let names = attached_names(source);
        for name in offered.split_whitespace() {
            assert_eq!(count_named(&names, name), 1, "{name} in {source}");
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
    assert_in_range(JSON, rebased, PRELUDE_LINE_BASE, is_prelude_line, &[]);
    assert!(
        attached(&rebased.program),
        "the prelude must recognise itself"
    );
    assert!(!is_prelude_line(PRELUDE_LINE_BASE) && is_prelude_line(PRELUDE_LINE_BASE + 1));
}

#[test]
fn later_prelude_units_rebase_cleanly_into_ranges_past_every_earlier_unit() {
    for (index, source, unit, earlier) in [
        (1, SIGNALS, signals(), &[json()][..]),
        (2, TESTING, testing(), &[signals()][..]),
        (3, TESTING_GPU, testing_gpu(), &[]),
    ] {
        let base = PRELUDE_LINE_BASE + index * PRELUDE_UNIT_STRIDE;
        assert_in_range(source, unit, base, |line| line >= base, earlier);
    }
    assert!(is_prelude_line(PRELUDE_LINE_BASE + PRELUDE_UNIT_STRIDE + 1));
    assert!(
        attached(&testing().program),
        "the testing unit must recognise itself"
    );
}

#[test]
fn a_program_that_names_or_declares_nothing_offered_is_left_exactly_as_written() {
    // Naming an internal primitive, or declaring its own prelude type or
    // constructor, does not pull the prelude in either.
    assert_left_as_written(&[
        "fn jsonish(x) = x\nprint(toString(jsonish(1)))\n",
        "print(toString(__jsonNodeKind(1, 0)))\n",
        "type JsonMember = JsonMember(string, int)\nprint(jsonEncode(JsonNull) ?: \"\")\n",
        "type Json = { raw: string }\nlet j = Json { raw: \"x\" }\n",
        "type Mine = JsonNull | Other\nlet m = JsonNull\n",
    ]);
}

#[test]
fn naming_any_offered_declaration_attaches_units_once_and_the_examples_type_check() {
    for (sources, offered, body) in [
        (
            &[
                "print(jsonEncode(JsonNull) ?: \"\")\n",
                "let decoded = jsonDecode(\"[]\")\n",
                "let member = JsonMember(\"k\", JsonInt(1))\n",
                "fn f(v) = jsonText(v)\n",
            ][..],
            "Json JsonMember jsonEncode jsonDecode jsonQuote jsonField jsonIndex jsonText jsonInt jsonFloat jsonBool",
            Some("let doc = JsonObject([JsonMember(\"a\", JsonArray([JsonInt(1), JsonFloat(2.5), JsonBool(true), JsonNull]))])\n\
                  let text = jsonEncode(doc) ?: \"\"\n\
                  let back = jsonDecode(text) ?: JsonNull\n\
                  let first = jsonField(back, \"a\") ?: JsonNull\n\
                  let one = jsonIndex(first, 0) ?: JsonNull\n\
                  print(\"${text} ${jsonQuote(\"q\") ?: \"\"} ${toString(jsonInt(one) ?: 0)} ${toString(jsonFloat(one) ?: 0.0)} ${toString(jsonBool(JsonBool(true)) ?: false)} ${jsonText(JsonString(\"s\")) ?: \"\"}\")\n"),
        ),
        (
            &[
                "fn slice() = (perform Signal<Route>.read()).tab\n",
                "let frames = signalForced([], \"route\")\n",
                "let frame = SignalFrame(\"route\", [], 1)\n",
                "let keyed = signalRefreshKeyed([], \"route\", [], \"\", fn() => \"x\")\n",
                "let changed = signalChanges(1, 2, toString, [])\n",
                "let text = jsonEncode(signalFramesJson([])) ?: \"\"\n",
            ][..],
            "Json JsonMember jsonEncode jsonDecode SignalFrame signalFrameKey signalFrame signalRefreshKeyed signalChanges signalFramesJson signalFramesFromJson",
            Some("type Route = { tab: string }\n\
                  fn label() = \"route:${(perform Signal<Route>.read()).tab}\"\n\
                  fn render(tab, frames, changed) = {\n\
                      handle static Signal<Route> { read => Route { tab: tab } }\n\
                      [signalFrame(signalRefreshKeyed(frames, \"route\", changed, \"\", label))]\n\
                  }\n\
                  let first = render(\"home\", [], [\"<unknown>\"])\n\
                  let away = Route { tab: \"away\" }\n\
                  let next = render(\"away\", first, signalChanges(Route { tab: \"home\" }, away, fn(route) => JsonString(route.tab), signalDependencies(label)))\n\
                  print(\"${toString(signalRevisionOf(first, \"route\"))}/${toString(signalRevisionOf(next, \"route\"))}\")\n"),
        ),
        (
            &[
                "test(\"w\", fn() => withBuffer(toGpu([1, 2]), fn(b) => expect(gpuLength(b), 2)))\n",
                "test(\"s\", fn() => checkTrue(gpuSatisfies(toGpu([3]), fn(b) => gpuLength(b) == 1)))\n",
                "test(\"i\", fn() => withIndices(4, fn(b) => expect(gpuLength(b), 4)))\n",
                "print(iotaError(0 - 1))\n",
                "test(\"e\", fn() => withIndices(2, fn(b) => checkTrue(elementIs(b, 1, 0, 1))))\n",
                "test(\"l\", fn() => withIndices(2, fn(b) => checkTrue(lengthIs(b, 2))))\n",
                "test(\"u\", fn() => checkTrue(emptyUploads()))\n",
                "print(oobMessage())\n",
                "test(\"f\", fn() => withIndices(3, fn(b) => checkTrue(elementsFrom(b, 1, 0, [1, 2]))))\n",
            ][..],
            "withBuffer gpuSatisfies withIndices iotaError",
            Some("test(\"all\", fn() => exactArithmetic(fn() => withIndices(3, fn(b) => checkAll(\"gpu\", [gpuSatisfies(gpuMap(b, fn(x) => x * 2), fn(d) => gpuLength(d) == 3), iotaError(0 - 1) != \"\", elementIs(b, 2, 0, 2), elementIs(b, 9, 7, 7), lengthIs(b, 3), emptyUploads(), oobMessage() != \"\", elementsFrom(b, 0, 0, [0, 1, 2]), !elementsFrom(b, 1, 0, [2])]))))\n"),
        ),
        (
            &[
                "test(\"a\", fn() => checkTrue(containsAll(\"alpha|beta\", [\"alpha\", \"beta\"])))\n",
                "test(\"n\", fn() => checkTrue(containsNone(\"alpha|beta\", [\"gamma\"])))\n",
            ][..],
            "containsAll containsNone exactArithmetic",
            Some("test(\"text\", fn() => checkAll(\"parts\", [containsAll(\"12|spare|9\", [\"12|\", \"|spare|\", \"|9\"]), containsNone(\"12|spare|9\", [\"2.5\", \"|13|\"]), !containsAll(\"ab\", [\"a\", \"c\"]), !containsNone(\"ab\", [\"b\"]), containsAll(\"\", []), containsNone(\"\", [])]))\n"),
        ),
    ] {
        assert_offers_attach_once(sources, offered);
        if let Some(body) = body {
            assert_type_checks(body, "");
        }
    }
}

#[test]
fn a_program_function_replaces_only_the_prelude_function_it_names() {
    let names = attached_names("fn jsonField(a, b) = a + b\nprint(jsonEncode(JsonNull) ?: \"\")\n");
    assert_program_shadow_wins(&names, "jsonField");
    assert!(
        names.iter().any(|name| name == "jsonQuote")
            && names.iter().any(|name| name == "__jsonMember")
    );
}

#[test]
fn naming_only_a_json_declaration_attaches_json_but_not_signals() {
    let names = attached_names("print(jsonEncode(JsonNull) ?: \"\")\n");
    assert_eq!(count_named(&names, "jsonEncode"), 1);
    assert_none_attached(
        &names,
        &["SignalFrame", "signalRefreshKeyed", "signalChanges"],
    );
}

#[test]
fn a_program_declaring_its_own_signal_effect_keeps_it_and_gets_no_signals_unit() {
    let effects = with_prelude(&program(
        "static effect Signal<T> { read: fn() -> T }\n\
         type Route = { tab: string }\n\
         fn slice() = (perform Signal<Route>.read()).tab\n\
         print(jsonEncode(JsonNull) ?: \"\")\n",
    ))
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
fn naming_the_testing_policy_attaches_it_once_and_no_json_or_signals() {
    let sources = [
        "test(\"sum\", fn() => exactArithmetic(fn() => expect(1 + 2, 3)))\n",
        "fn case() = check(\"q\", \"2\", toString(7 / 3))\ntest(\"q\", fn() => exactArithmetic(case))\n",
        "let policy = exactArithmetic\npolicy(fn() => print(toString(9 * 9)))\n",
        "exactArithmetic fn () => print (toString (2 + 2))\n",
    ];
    assert_offers_attach_once(&sources[..3], "exactArithmetic");
    for source in &sources[..3] {
        assert_none_attached(&attached_names(source), &["Json", "SignalFrame"]);
    }
}

#[test]
fn a_program_declaring_its_own_testing_policy_keeps_its_own() {
    let names = attached_names(
        "fn exactArithmetic(work) = work()\ntest(\"own\", fn() => exactArithmetic(fn() => expect(1, 1)))\n",
    );
    assert_program_shadow_wins(&names, "exactArithmetic");
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
        let source = format!(
            "fn rest(n) = 7 % n\ntest(\"faults\", fn() => exactArithmetic(fn() => {body}))\n"
        );
        assert_type_checks(&source, body);
    }
    let unhandled = program("fn rest(n) = 7 % n\ntest(\"bare\", fn() => expect(rest(3), 1))\n");
    assert!(
        !crate::check_program(&unhandled).is_empty(),
        "without the policy the program is rejected"
    );
}

/// A program whose own union declares `Success` names only the arithmetic
/// policy: it gets that unit and none of the GPU helpers, whose `Success`
/// patterns its variant would capture. [TESTING-ARITH-POLICY]
#[test]
fn a_program_with_its_own_success_variant_gets_only_the_testing_unit_it_names() {
    let source = "type Outcome = Success | Failed\n\
         fn score(outcome) = match outcome {\n    Success => 1\n    Failed => 0\n}\n\
         fn main() = exactArithmetic(fn() => print(toString(score(Success))))\n";
    let names = attached_names(source);
    assert!(names.contains(&"exactArithmetic".to_string()), "{names:?}");
    assert_none_attached(
        &names,
        &["withBuffer", "gpuSatisfies", "withIndices", "iotaError"],
    );
    assert_type_checks(source, "");
    let gpu = attached_names(
        "fn main() = withIndices(4, fn(buffer) => print(toString(gpuLength(buffer))))\n",
    );
    assert!(
        gpu.contains(&"withBuffer".to_string()) && gpu.contains(&"withIndices".to_string()),
        "{gpu:?}"
    );
}
