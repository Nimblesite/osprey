use super::*;

/// Assert the one runtime-list seal in `main` precedes its first `marker`.
fn sealed_before(main: &str, marker: &str) {
    let sealed = main
        .find("call i8* @osprey_list_builder_seal")
        .unwrap_or(main.len());
    let published = main.find(marker).unwrap_or(0);
    assert!(sealed < published, "seal after {marker}:\n{main}");
}

/// The `@main` body, asserting it makes exactly one `needle` call.
fn main_calls_once(ir: &str, needle: &str) -> String {
    let main = function_body(ir, "define i32 @main()");
    assert_eq!(main.matches(needle).count(), 1, "{needle} in:\n{main}");
    main
}

/// Assert `text` contains `needle` exactly `n` times.
fn assert_calls(text: &str, needle: &str, n: usize) {
    assert_eq!(text.matches(needle).count(), n, "{needle} in:\n{text}");
}

/// A program whose whole body is one `fn main() -> Unit`.
fn main_body(body: &str) -> String {
    module(&["fn main() -> Unit = {\n", body, "\n}\n"].concat())
}

/// Source for an `inspect` operation over a Result/fiber pair, in the direct
/// or the `resuming` (`control`) ABI; `checkedAdd` supplies the failures.
fn inspect_handler_source(effect: &str, resuming: bool) -> String {
    let (control, success, failed) = if resuming {
        ("control ", "resume(value)", "resume(7)")
    } else {
        ("", "value", "7")
    };
    format!(
        "effect {effect} {{ {control}inspect: fn(Result<int, Error>, Fiber<Result<int, Error>>) -> int }}\n\
         fn ask() -> int !{effect} = perform {effect}.inspect(checkedAdd(9223372036854775807, 1), spawn(checkedAdd(9223372036854775807, 1)))\n\
         fn main() -> int = {{\n\
             handle {effect} {{\n\
                 inspect immediate deferred => match immediate {{\n\
                 Success {{ value }} => {success}\n\
                 Error {{ message }} => match await(deferred) {{\n\
                 Success {{ value }} => {success}\n\
                 Error {{ message }} => {failed}\n\
                 }}\n\
                 }}\n\
             }}\n\
             ask()\n\
         }}\n"
    )
}

#[test]
fn allocation_masks_follow_the_targets_mixed_record_layout() {
    let program = parsed(
        "type Mixed = { a: string, b: string, scalar: int, c: string }\n\
         fn inspect(value) = print(\"${value.a}${value.b}${value.scalar}${value.c}\")\n\
         inspect(Mixed { a: \"a\", b: \"b\", scalar: 42, c: \"c\" })\n",
        Flavor::Default,
    );
    for (result, expected) in [
        (compile_program(&program), "i64 5633)"),
        (compile_wasm_program(&program), "i64 19457)"),
    ] {
        let ir = result.expect("mixed record compiles");
        assert!(
            ir.contains(expected),
            "missing target pointer mask {expected}: {ir}"
        );
    }
}

#[test]
fn match_result_constructor_arms_share_the_contextual_success_layout() {
    // A bare Error constructor initially has a string-shaped placeholder
    // success slot.  The match join must re-layout it to the Success arm's
    // inferred payload type, regardless of source-arm order, so the whole
    // Result remains readable on both native and wasm32 targets.
    for body in [
        "\
               \"known\" => Success { value: 4 }\n\
               _ => Error { message: \"absent\" }",
        "\
               \"missing\" => Error { message: \"absent\" }\n\
               _ => Success { value: 4 }",
    ] {
        let ir = module(&format!(
            "fn find(key: string) -> Result<int, string> = match key {{\n{body}\n}}\n\
                 fn main() -> Unit = print(\"${{find(\"known\")}}\")\n"
        ));
        shows(&ir, &["phi { i64, i8, i8* }*"]);
    }
}

#[test]
fn result_parameters_preserve_the_wrapper_for_named_functions_and_closures() {
    // `checkedAdd` is the arithmetic that still returns overflow as a
    // Result value ([ARITH-EFFECT]).
    let body = "match r {\n\
                Success { value } => value\n\
                Error { message } => 0\n\
              }\n\
              let failed = choose(checkedAdd(9223372036854775807, 1))\n\
              let wrapped = choose(7)\n\
              print(\"${failed}:${wrapped}\")\n";
    for (head, expected) in [
        (
            "fn choose(r: Result<int, Error>) -> int = ",
            &[
                "define internal i64 @choose(i8* %$p0)",
                "bitcast i8* %$p0 to { i64, i8, i8* }*",
            ][..],
        ),
        (
            "let choose = fn(r: Result<int, Error>) => ",
            &[
                "define internal i64 @__closure_fn_",
                "i8* %__env, i8* %$p0",
                "bitcast i8* %$p0 to { i64, i8, i8* }*",
            ][..],
        ),
    ] {
        shows(&module(&[head, body].concat()), expected);
    }
}

/// Matching a bare scalar against `Success`/`Error` arms takes the Success
/// arm UNCONDITIONALLY, per the auto-wrap rule ("any value may be matched as
/// if wrapped in `Success`", osprey-types/src/pattern.rs).
///
/// This used to emit `icmp sge i64 disc, 0` and route a NEGATIVE scalar to
/// the Error arm — so `-1 ?: 99` silently produced `99`. The sign of a value
/// must never decide which arm runs. Implements [PATTERN-RESULT-AUTOWRAP].
#[test]
fn result_match_on_a_scalar_discriminant_always_takes_success() {
    for scrutinee in ["5", "-1"] {
        let ir = module(&format!(
            "fn main() -> Unit = {{\n\
                   let n = {scrutinee}\n\
                   match n {{\n\
                     Success {{ value }} => print(\"v=${{value}}\")\n\
                     Error {{ message }} => print(\"e=${{message}}\")\n\
                   }}\n\
                 }}\n"
        ));
        testkit::contents(&ir, &[], &["icmp sge i64"]);
        shows(&ir, &["br i1 true"]);
    }
}

#[test]
fn pattern_lowering_binds_and_compares() {
    // A union match whose catch-all is a binding (not `_`) binds the whole
    // scrutinee (pattern.rs gen_union_match's Binding catch-all arm). A
    // string-literal compare/branch chain ending in a catch-all is
    // pattern.rs gen_literal_match + gen_eq's osp_string_compare path.
    for (src, needles) in [
        (
            "type Shape = Circle { r: int } | Square { s: int } | Blank\n\
             fn name(sh: Shape) -> int = match sh {\n\
               Circle { r } => r\n\
               other        => 0\n\
             }\n\
             fn main() -> Unit = print(\"${name(Blank)}\")\n",
            &["load i64, i64*"][..],
        ),
        (
            "fn route(p: string) -> string = match p {\n\
               \"/a\" => \"A\"\n\
               \"/b\" => \"B\"\n\
               _     => \"404\"\n\
             }\n\
             fn main() -> Unit = print(route(\"/a\"))\n",
            &["@osp_string_compare"][..],
        ),
    ] {
        shows(&module(src), needles);
    }
}

// ---- strings ----

#[test]
fn string_builtins_total_and_fallible() {
    // A broad sweep of string builtins: total transforms, predicates,
    // fallible parse/substring/split, cursor ops, fromCodePoint, join.
    let ir = main_body(
        "let s = \"  Hello World  \"\n\
         print(\"${length(s)} ${isEmpty(s)} ${contains(s, \"World\")}\")\n\
         print(\"${startsWith(s, \"  \")} ${endsWith(s, \"  \")}\")\n\
         print(\"${toUpperCase(s)} ${toLowerCase(s)} ${trim(s)}\")\n\
         print(\"${trimStart(s)} ${trimEnd(s)} ${reverse(s)}\")\n\
         print(\"${take(s, 2)} ${drop(s, 2)}\")\n\
         print(\"${repeat(\"ab\", 3)} ${padStart(\"x\", 3, \"-\")} ${padEnd(\"x\", 3, \"-\")}\")\n\
         print(\"${replace(\"aaa\", \"a\", \"b\")} ${byteLength(s)}\")\n\
         match indexOf(s, \"World\") { Success { value } => print(\"i=${value}\") Error { message } => print(\"no\") }\n\
         match substring(s, 2, 5) { Success { value } => print(\"sub=${value}\") Error { message } => print(\"no\") }\n\
         match parseInt(\"42\") { Success { value } => print(\"n=${value}\") Error { message } => print(\"no\") }\n\
         match parseFloat(\"4.5\") { Success { value } => print(\"f=${value}\") Error { message } => print(\"no\") }\n\
         match split(\"a,b,c\", \",\") { Success { value } => print(\"parts=${listLength(value)}\") Error { message } => print(\"no\") }\n\
         match byteAt(s, 0) { Success { value } => print(\"b=${value}\") Error { message } => print(\"no\") }\n\
         match codePointAt(s, 0) { Success { value } => print(\"cp=${value}\") Error { message } => print(\"no\") }\n\
         match fromCodePoint(65) { Success { value } => print(\"c=${value}\") Error { message } => print(\"no\") }\n\
         let ws = words(s)\n\
         let ls = lines(s)\n\
         print(\"${join(ws, \"-\")} ${listLength(ls)}\")",
    );
    shows(
        &ir,
        &[
            "@osp_strlen",
            "osp_string_to_upper",
            "osp_parse_int_strict",
            "osp_string_codepoint_at",
            "osp_string_join",
        ],
    );
}

// ---- collections: map literals, map operations ----

#[test]
fn map_literal_and_map_operations() {
    // Map literal build, set/get/contains/remove/merge, keys/values lists,
    // indexing — collections.rs gen_map_literal, map_* and map_to_list.
    let ir = main_body(
        "let m = { \"a\": 1, \"b\": 2 }\n\
         let m2 = mapSet(m, \"c\", 3)\n\
         let m3 = mapRemove(m2, \"a\")\n\
         let merged = m2 + m3\n\
         print(\"len=${mapLength(merged)} has=${mapContains(m2, \"a\")}\")\n\
         print(\"keys=${listLength(mapKeys(m2))} vals=${listLength(mapValues(m2))}\")\n\
         match mapGet(m2, \"b\") { Success { value } => print(\"g=${value}\") Error { message } => print(\"no\") }\n\
         match m[\"a\"] { Success { value } => print(\"i=${value}\") Error { message } => print(\"no\") }",
    );
    shows(
        &ir,
        &[
            "osprey_map_builder_new",
            "osprey_map_set",
            "osprey_map_remove",
            "osprey_map_iter_new",
        ],
    );
}

#[test]
fn bare_length_and_is_empty_dispatch_on_the_receiver_type() {
    // [BUILTIN-COLLECTION-LENGTH], [BUILTIN-COLLECTION-ISEMPTY]: the bare
    // spec names are receiver-directed. Routing a List/Map handle into the
    // string runtime reads an `i8*` heap pointer as a NUL-terminated string —
    // a wrong answer AND an out-of-bounds read. A flat list *literal* is a
    // third layout: it matched neither collection tag, fell through to
    // `osp_strlen`, and answered `1` for every list of 1..=255 elements.
    let ir = main_body(
        "let xs = listAppend(listAppend(List(), 1), 2)\n\
         let m = mapSet(Map(), \"a\", 1)\n\
         let lit = [7, 8, 9]\n\
         print(\"${length(xs)} ${isEmpty(xs)} ${length(m)} ${isEmpty(m)} ${length(lit)} ${isEmpty(lit)} ${length(\"ab\")}\")",
    );
    shows(
        &ir,
        &["osprey_list_length", "osprey_map_length", "osp_strlen"],
    );
    // Exactly one `osp_strlen` call site: the sole string receiver, and no
    // collection receiver may reach the string isEmpty.
    assert_calls(&ir, "call i64 @osp_strlen", 1);
    assert_calls(&ir, "@osp_string_is_empty", 0);
}

#[test]
fn list_literal_operands_of_plus_are_rebuilt_before_concatenation() {
    // `+` on lists is `listConcat`, selected by the LIST_OWNER tag — which a
    // flat list literal does not carry: `xs + [1]` handed `osprey_list_concat`
    // the literal's foreign `{ i64, i8* }` header (a SEGFAULT) and `[1] + [2]`
    // fell through to integer arithmetic. Both are now rebuilt as runtime lists.
    let ir = main_body(
        "let xs = listAppend(List(), 1)\n\
         let both = [1, 2] + [3]\n\
         let mixedL = xs + [4]\n\
         let mixedR = [5] + xs\n\
         print(\"${listLength(both)} ${listLength(mixedL)} ${listLength(mixedR)}\")",
    );
    // Three concatenations, each over two real OspreyList handles.
    assert_calls(&ir, "call i8* @osprey_list_concat", 3);
    // Five literals appear; each is sealed into a runtime list before use.
    assert!(
        ir.matches("osprey_list_builder_seal").count() >= 5,
        "each literal operand must be rebuilt as a runtime list"
    );
}

#[test]
fn a_list_literal_argument_is_rebuilt_before_it_crosses_into_a_callee() {
    // A callee's `List<T>` parameter is one `i8*` whichever layout the caller
    // wrote, so the body cannot branch on it. `osprey_list_length` reads both
    // layouts (shared leading `i64`), but `osprey_list_get`/`_drop` need the
    // real trie — a list pattern over a literal argument SEGFAULTED while a
    // `listAppend` chain worked, so the literal is rebuilt at the boundary.
    let ir = module(
        "fn headOf(xs: List<int>) -> int = match xs {\n\
               [] => 0\n\
               [h, ...t] => h\n\
             }\n\
             fn main() -> Unit = {\n\
               let rt = listAppend(List(), 7)\n\
               print(\"${headOf([7, 8])} ${headOf(rt)}\")\n\
             }\n",
    );
    // The literal argument seals a runtime list; the handle argument does not.
    assert_calls(&ir, "call i8* @osprey_list_builder_seal", 1);
    shows(&ir, &["osprey_list_get"]);
}

#[test]
fn a_file_scope_list_literal_is_rebuilt_before_a_function_reads_it() {
    // A module global is the same boundary as a parameter: the reading
    // function sees one `List<T>` `i8*`. Publishing the flat literal as-is
    // made a list pattern, `big[1]` or `toGpu(big)` inside a function walk
    // the literal's data pointer as a trie and SEGFAULT, while `listLength`
    // appeared to work (shared leading `i64`). [MODULES-FILE-SCOPE-BINDING]
    let ir = module(
        "let big = [1, 2]\n\
             fn first() = match big {\n\
               [] => 0\n\
               [h, ...rest] => h\n\
             }\n\
             print(first())\n",
    );
    sealed_before(&function_body(&ir, "define i32 @main()"), "@osp.g.big");
}

#[test]
fn a_list_literal_fold_seed_is_rebuilt_once_before_the_loop() {
    // `fold([], …)` carries its seed through the loop-carried slot. Left flat,
    // the second iteration read the runtime list `listAppend` handed back as a
    // `{ length, data }` header and SEGFAULTED. The seed now takes the runtime
    // layout once, at the escape into the slot, and the loop body converts
    // nothing per iteration [BUILTIN-ITER-FOLD].
    let ir = main_body(
        "let xs = range(0, 3) |> fold([], fn(acc, i) => listAppend(acc, i))\n\
         print(listLength(xs))",
    );
    let main = main_calls_once(&ir, "call i8* @osprey_list_builder_new_of");
    sealed_before(&main, "@osprey_list_append");
}

#[test]
fn a_bound_string_list_is_settled_once_at_its_binding() {
    // `let rows = lines(s)` then `listGet(rows, i)` in a loop copied the WHOLE
    // list on every read (50 000 lines × 5 000 reads: 6.9 GB and nine seconds).
    // The binding settles the `[]i8*` result once; the loop reads the runtime
    // list in place [BUILTIN-LIST-GET].
    let ir = main_body(
        "let rows = lines(\"a\\nb\\nc\")\n\
         let total = range(0, 3) |> fold(0, fn(acc, i) => wrapAdd(acc, length(listGet(rows, i) ?: \"\")))\n\
         print(total)",
    );
    let main = main_calls_once(&ir, "call i8* @osprey_list_builder_new_of(i32 1)");
    sealed_before(&main, "@osprey_list_in_bounds");
}

#[test]
fn list_get_and_contains_runtime_calls() {
    // listGet (bounds-checked Result) and listContains (linear scan with
    // both the int and string equality paths).
    let ir = main_body(
        "let xs = listAppend(listAppend(List(), 1), 2)\n\
         let ss = listAppend(List(), \"hi\")\n\
         print(\"c=${listContains(xs, 2)} s=${listContains(ss, \"hi\")}\")\n\
         match listGet(xs, 0) { Success { value } => print(\"v=${value}\") Error { message } => print(\"no\") }",
    );
    shows(&ir, &["osprey_list_in_bounds", "@osp_string_compare"]);
}

// ---- algebraic effects: handler-owned state (effects.rs) ----

#[test]
fn effect_result_parameters_preserve_shape_in_direct_and_resuming_abis() {
    fn try_module(src: &str) -> std::result::Result<String, String> {
        let parsed = parse_program(src);
        if !parsed.errors.is_empty() {
            return Err(format!("syntax errors: {:?}", parsed.errors));
        }
        compile_program(&parsed.program).map_err(|error| error.to_string())
    }

    for (effect, resuming, function) in [
        ("Inspect", false, "@__handler_Inspect_inspect_"),
        (
            "ResumeInspect",
            true,
            "@__resume_arm_ResumeInspect_inspect_",
        ),
    ] {
        let compiled = try_module(&inspect_handler_source(effect, resuming));
        assert!(
            compiled.is_ok(),
            "effect Result parameters must compile without erasing their wrapper:\n  {effect}={compiled:?}"
        );
        let ir = compiled.expect("checked above");
        let definition = ir
            .lines()
            .find(|line| line.starts_with("define ") && line.contains(function))
            .expect("effect handler definition");
        // Arm registers are POSITIONAL (`%__arm0`, `%__arm1`), never the
        // source binders: a binder spelled `entry` collided with the
        // `entry:` block label and clang refused the module. The SHAPES
        // are the contract — the Result parameter travels as its block
        // pointer, the fiber as its id word ([FLAVOR-IR-EQUIV]).
        assert!(
            definition.contains("i8* %__arm0") && definition.contains("i64 %__arm1"),
            "effect parameters must use their complete ParamSig ABI:\n{definition}\n{ir}"
        );
        shows(&ir, &["bitcast i8* %__arm0 to { i64, i8, i8* }*"]);
        assert!(
            ir.lines()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|pair| matches!(pair, [first, second]
                        if first.contains("inttoptr i64")
                            && first.contains("to i8*")
                            && second.contains("bitcast i8*")
                            && second.contains("to { i64, i8, i8* }*"))),
            "the Fiber<Result> parameter must retain its Result element shape:\n{ir}"
        );
    }
}
