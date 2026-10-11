use super::*;

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
        assert!(ir.contains("phi { i64, i8, i8* }*"), "unexpected IR:\n{ir}");
    }
}

#[test]
fn result_parameters_preserve_the_wrapper_for_named_functions_and_closures() {
    // `checkedAdd` is the arithmetic that still returns overflow as a
    // Result value ([ARITH-EFFECT]).
    let named = module(
        "fn choose(r: Result<int, Error>) -> int = match r {\n\
               Success { value } => value\n\
               Error { message } => 0\n\
             }\n\
             let failed = choose(checkedAdd(9223372036854775807, 1))\n\
             let wrapped = choose(7)\n\
             print(\"${failed}:${wrapped}\")\n",
    );
    assert!(
        named.contains("define i64 @choose(i8* %$p0)"),
        "unexpected named-function IR:\n{named}"
    );
    assert!(named.contains("bitcast i8* %$p0 to { i64, i8, i8* }*"));

    let closure = module(
        "let choose = fn(r: Result<int, Error>) => match r {\n\
               Success { value } => value\n\
               Error { message } => 0\n\
             }\n\
             let failed = choose(checkedAdd(9223372036854775807, 1))\n\
             let wrapped = choose(7)\n\
             print(\"${failed}:${wrapped}\")\n",
    );
    shows(
        &closure,
        &[
            "define i64 @__closure_fn_",
            "i8* %__env, i8* %$p0",
            "bitcast i8* %$p0 to { i64, i8, i8* }*",
        ],
    );
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
        assert!(
            !ir.contains("icmp sge i64"),
            "the sign of {scrutinee} must not select the arm:\n{ir}"
        );
        assert!(
            ir.contains("br i1 true"),
            "Success arm is unconditional for {scrutinee}:\n{ir}"
        );
    }
}

#[test]
fn union_match_with_named_catch_all_binds_the_scrutinee() {
    // A union match whose catch-all is a binding (not `_`) binds the whole
    // scrutinee (pattern.rs gen_union_match's Binding catch-all arm).
    let ir = module(
        "type Shape = Circle { r: int } | Square { s: int } | Blank\n\
             fn name(sh: Shape) -> int = match sh {\n\
               Circle { r } => r\n\
               other        => 0\n\
             }\n\
             fn main() -> Unit = print(\"${name(Blank)}\")\n",
    );
    assert!(ir.contains("load i64, i64*"));
}

#[test]
fn string_literal_match_chain() {
    // A string-literal compare/branch chain ending in a catch-all
    // (pattern.rs gen_literal_match + gen_eq's strcmp path).
    let ir = module(
        "fn route(p: string) -> string = match p {\n\
               \"/a\" => \"A\"\n\
               \"/b\" => \"B\"\n\
               _     => \"404\"\n\
             }\n\
             fn main() -> Unit = print(route(\"/a\"))\n",
    );
    assert!(ir.contains("@strcmp"));
}

// ---- strings ----

#[test]
fn string_builtins_total_and_fallible() {
    // A broad sweep of string builtins: total transforms, predicates,
    // fallible parse/substring/split, cursor ops, fromCodePoint, join.
    let ir = module(
            "fn main() -> Unit = {\n\
               let s = \"  Hello World  \"\n\
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
               print(\"${join(ws, \"-\")} ${listLength(ls)}\")\n\
             }\n",
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
    let ir = module(
            "fn main() -> Unit = {\n\
               let m = { \"a\": 1, \"b\": 2 }\n\
               let m2 = mapSet(m, \"c\", 3)\n\
               let m3 = mapRemove(m2, \"a\")\n\
               let merged = m2 + m3\n\
               print(\"len=${mapLength(merged)} has=${mapContains(m2, \"a\")}\")\n\
               print(\"keys=${listLength(mapKeys(m2))} vals=${listLength(mapValues(m2))}\")\n\
               match mapGet(m2, \"b\") { Success { value } => print(\"g=${value}\") Error { message } => print(\"no\") }\n\
               match m[\"a\"] { Success { value } => print(\"i=${value}\") Error { message } => print(\"no\") }\n\
             }\n",
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
    // string runtime reads an `i8*` heap pointer as a NUL-terminated string
    // — a wrong answer AND an out-of-bounds read. A flat list *literal* is a
    // third layout: it matched neither collection tag and fell through to
    // `osp_strlen`, which counted the length word's own bytes and answered
    // `1` for every list of 1..=255 elements.
    let ir = module(
            "fn main() -> Unit = {\n\
               let xs = listAppend(listAppend(List(), 1), 2)\n\
               let m = mapSet(Map(), \"a\", 1)\n\
               let lit = [7, 8, 9]\n\
               print(\"${length(xs)} ${isEmpty(xs)} ${length(m)} ${isEmpty(m)} ${length(lit)} ${isEmpty(lit)} ${length(\"ab\")}\")\n\
             }\n",
        );
    assert!(
        ir.contains("osprey_list_length"),
        "List length must use the list runtime"
    );
    assert!(
        ir.contains("osprey_map_length"),
        "Map length must use the map runtime"
    );
    assert!(
        ir.contains("osp_strlen"),
        "string length must still use the string runtime"
    );
    // Exactly one `osp_strlen` call site: the sole string receiver.
    assert_eq!(
        ir.matches("call i64 @osp_strlen").count(),
        1,
        "only the string receiver may reach osp_strlen"
    );
    assert_eq!(
        ir.matches("@osp_string_is_empty").count(),
        0,
        "no collection receiver may reach the string isEmpty"
    );
}

#[test]
fn list_literal_operands_of_plus_are_rebuilt_before_concatenation() {
    // `+` on lists is `listConcat`, selected by the LIST_OWNER tag — which a
    // flat list literal does not carry. `xs + [1]` therefore handed
    // `osprey_list_concat` the literal's foreign `{ i64, i8* }` header (a
    // SEGFAULT), and `[1] + [2]` matched neither operand and fell through to
    // integer arithmetic. Both operands are now rebuilt into runtime lists.
    let ir = module(
        "fn main() -> Unit = {\n\
               let xs = listAppend(List(), 1)\n\
               let both = [1, 2] + [3]\n\
               let mixedL = xs + [4]\n\
               let mixedR = [5] + xs\n\
               print(\"${listLength(both)} ${listLength(mixedL)} ${listLength(mixedR)}\")\n\
             }\n",
    );
    // Three concatenations, each over two real OspreyList handles.
    assert_eq!(
        ir.matches("call i8* @osprey_list_concat").count(),
        3,
        "every `+` over a list must reach the list runtime"
    );
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
    // real trie — so a list pattern over a literal argument SEGFAULTED while
    // the same call on a `listAppend` chain worked. The literal is rebuilt at
    // the boundary instead.
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
    assert_eq!(
        ir.matches("call i8* @osprey_list_builder_seal").count(),
        1,
        "exactly the literal argument is rebuilt"
    );
    assert!(
        ir.contains("osprey_list_get"),
        "the arm still binds its head from the list runtime"
    );
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
    let main = function_body(&ir, "define i32 @main()");
    let sealed = main.find("call i8* @osprey_list_builder_seal");
    let published = main.find("@osp.g.big");
    assert!(
        sealed
            .zip(published)
            .is_some_and(|(seal, store)| seal < store),
        "the literal must be sealed into a runtime list before it is published:\n{main}"
    );
}

#[test]
fn list_get_and_contains_runtime_calls() {
    // listGet (bounds-checked Result) and listContains (linear scan with
    // both the int and string equality paths).
    let ir = module(
            "fn main() -> Unit = {\n\
               let xs = listAppend(listAppend(List(), 1), 2)\n\
               let ss = listAppend(List(), \"hi\")\n\
               print(\"c=${listContains(xs, 2)} s=${listContains(ss, \"hi\")}\")\n\
               match listGet(xs, 0) { Success { value } => print(\"v=${value}\") Error { message } => print(\"no\") }\n\
             }\n",
        );
    shows(&ir, &["osprey_list_in_bounds", "@strcmp"]);
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

    // `checkedAdd` supplies the failed Result values ([ARITH-EFFECT]).
    let direct = try_module(
            "effect Inspect { inspect: fn(Result<int, Error>, Fiber<Result<int, Error>>) -> int }\n\
             fn ask() -> int !Inspect = perform Inspect.inspect(checkedAdd(9223372036854775807, 1), spawn(checkedAdd(9223372036854775807, 1)))\n\
             fn main() -> int = {\n\
                 handle Inspect {\n\
                     inspect immediate deferred => match immediate {\n\
                     Success { value } => value\n\
                     Error { message } => match await(deferred) {\n\
                     Success { value } => value\n\
                     Error { message } => 7\n\
                     }\n\
                     }\n\
                 }\n\
                 ask()\n\
             }\n",
        );
    let resuming = try_module(
            "effect ResumeInspect { control inspect: fn(Result<int, Error>, Fiber<Result<int, Error>>) -> int }\n\
             fn ask() -> int !ResumeInspect = perform ResumeInspect.inspect(checkedAdd(9223372036854775807, 1), spawn(checkedAdd(9223372036854775807, 1)))\n\
             fn main() -> int = {\n\
                 handle ResumeInspect {\n\
                     inspect immediate deferred => match immediate {\n\
                     Success { value } => resume(value)\n\
                     Error { message } => match await(deferred) {\n\
                     Success { value } => resume(value)\n\
                     Error { message } => resume(7)\n\
                     }\n\
                     }\n\
                 }\n\
                 ask()\n\
             }\n",
        );
    assert!(
            direct.is_ok() && resuming.is_ok(),
            "effect Result parameters must compile without erasing their wrapper:\n  direct={direct:?}\n  resuming={resuming:?}"
        );

    for (ir, function) in [
        (
            direct.expect("checked above"),
            "@__handler_Inspect_inspect_",
        ),
        (
            resuming.expect("checked above"),
            "@__resume_arm_ResumeInspect_inspect_",
        ),
    ] {
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
        assert!(
            ir.contains("bitcast i8* %__arm0 to { i64, i8, i8* }*"),
            "the handler must reconstruct the incoming Result block:\n{ir}"
        );
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
