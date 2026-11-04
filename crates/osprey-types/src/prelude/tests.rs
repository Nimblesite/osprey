use super::*;
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
    assert_all_lines(&statement_lines(rebased, type_position), is_prelude_line);
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
