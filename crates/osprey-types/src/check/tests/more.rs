use super::*;

#[test]
fn testing_builtins_typecheck_and_reject_bad_arity() {
    // [TESTING-BUILTINS] all assertion schemes accept the documented shapes.
    let errs = check(
        "test(\"adds\", fn() => expect(1 + 1, 2))\n\
         test(\"labeled\", fn() => check(\"sum\", 4, 2 + 2))\n\
         test(\"predicates\", fn() => {\n\
           expectTrue(2 < 3)\n\
           expectFalse(3 < 2)\n\
           checkTrue(\"ordered\", 4 <= 4)\n\
           checkFalse(\"different\", 4 == 5)\n\
           expectAll([true, 2 < 3, 4 == 4])\n\
           checkAll(\"batch\", [true, 5 != 6])\n\
         })\n",
    );
    assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
    let errs = check("expect(1)\n");
    assert!(errs.iter().any(|e| e.message.contains("arity")));
    let errs = check("expectTrue(1)\n");
    assert!(errs.iter().any(|e| e.message.contains("type mismatch")));
    let errs = check("checkFalse(true)\n");
    assert!(errs.iter().any(|e| e.message.contains("arity")));
    let errs = check("expectAll([true, 1])\n");
    assert!(errs.iter().any(|e| e.message.contains("type mismatch")));
    let errs = check("checkAll(\"batch\", [true])\n");
    assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
    let errs = check("test(42, fn() => expect(1, 1))\n");
    assert!(
        !errs.is_empty(),
        "a non-string test name must not typecheck"
    );
}

#[test]
fn testing_builtins_are_shadowable_but_others_stay_reserved() {
    // [TESTING-SHADOWING] a user test/expect/check replaces the built-in;
    // every other built-in still rejects redefinition.
    let errs = check(
        "fn check(t: int) -> int = wrapAdd(t, 1)\n\
         fn expect(a: int) -> int = a\n\
         fn test(x: int) -> int = x\n\
         let r = check(expect(test(1)))\n",
    );
    assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
    bad_with("fn range(t: int) -> int = t\n", "cannot redefine built-in");
}

#[test]
fn receiver_directed_size_builtins_reject_unsupported_types() {
    // [BUILTIN-COLLECTION-LENGTH] [BUILTIN-COLLECTION-ISEMPTY]
    ok("let xs = [1, 2]\n\
        let scores = { \"alice\": 1 }\n\
        let a = length(\"abc\")\n\
        let b = xs.length()\n\
        let c = scores |> isEmpty\n");
    ok("fn has_bytes(source) = length(source) > 0 && byteLength(source) > 0\n");
    for source in [
        "let n = length(42)\n",
        "let n = isEmpty(false)\n",
        "type R = { value: int }\nlet n = length(R { value: 1 })\n",
        "let n = isEmpty(fn(x) => x)\n",
    ] {
        let errs = check(source);
        assert!(
            errs.iter().any(|e| e
                .message
                .contains("supports only string, List<T>, or Map<string, V>")),
            "expected receiver rejection for {source:?}, got {errs:?}"
        );
    }
}

#[test]
fn string_conversion_builtins_reject_runtime_handles() {
    // [BUILTIN-PRINT] [BUILTIN-TOSTRING]
    ok("fn noop() -> Unit = sleep(0)\n\
        let a = print(1)\n\
        let b = print(1.5)\n\
        let c = print(true)\n\
        let d = print(\"text\")\n\
        let e = print(noop())\n\
        let f = toString(intDiv(4, 2))\n\
        extern fn textResult() -> Result<string, string>\n\
        let g = print(textResult())\n");
    for source in [
        "type R = { value: int }\nlet s = toString(R { value: 1 })\n",
        "let s = print([1, 2])\n",
        "let m = { \"one\": 1 }\nlet s = toString(m)\n",
        "let s = print(fn(x) => x)\n",
        "let s = toString(range(0, 2))\n",
        "let s = print(spawn 1)\n",
        "let c = Channel(1)\nlet s = toString(c)\n",
        "extern fn raw() -> Ptr\nlet s = print(raw())\n",
        "extern fn bad() -> Result<List<int>, Error>\nlet s = toString(bad())\n",
        "type R = { value: int }\nextern fn bad() -> Result<int, R>\nlet s = print(bad())\n",
    ] {
        let errs = check(source);
        assert!(
            errs.iter()
                .any(|e| e.message.contains("cannot convert value")),
            "expected string-conversion rejection for {source:?}, got {errs:?}"
        );
    }
}

#[test]
fn runtime_callbacks_use_their_exact_function_types() {
    ok("fn event(pid: int, kind: int, data: string) -> Unit = print(data)\n\
        let process = spawnProcess(\"echo ok\", event)\n\
        fn request(method: string, path: string, headers: string, body: string) -> HttpResponse = HttpResponse { status: 200, headers: headers, contentType: \"text/plain\", streamFd: -1, isComplete: true, partialBody: body }\n\
        let listening = httpListen(1, request)\n");
    let process_errs = check(
        "fn wrong(pid: int) -> Unit = print(pid)\n\
         let process = spawnProcess(\"echo no\", wrong)\n",
    );
    assert!(process_errs
        .iter()
        .any(|e| e.message.contains("function arity mismatch")));
    let http_errs = check(
        "fn wrong(method: string, path: string, headers: string, body: string) = body\n\
         let listening = httpListen(1, wrong)\n",
    );
    assert!(http_errs
        .iter()
        .any(|e| e.message.contains("type mismatch")));
}

#[test]
fn declared_typaram_functions_generalize_regardless_of_binding_direction() {
    // Regression lock for the builtin binder-id collision: builtin schemes
    // quantify hand-written Var(0)/Var(1); when the fresh supply also
    // handed out those ids, a var-var unification routed through them made
    // `TypeEnv::free_vars` resolve THROUGH a builtin's binder, and
    // `fn identity<T>(x) -> T = x` silently lost its polymorphism (the
    // failure depended on which side of the unification held the typaram
    // var). All three annotation spellings must stay polymorphic across
    // two instantiations. See `builtins::RESERVED_SCHEME_VARS`.
    ok("fn id1<T>(x: T) -> T = x\n\
        print(\"${id1(5)} ${id1(\"hi\")}\")\n");
    ok("fn id2<T>(x: T) = x\n\
        print(\"${id2(5)} ${id2(\"hi\")}\")\n");
    ok("fn id3<T>(x) -> T = x\n\
        print(\"${id3(5)} ${id3(\"hi\")}\")\n");
    // The HOF shape that first exposed it: a generic fn passed by name to
    // an inferred HOF, applied at two different instantiations.
    ok("fn identity<T>(x: T) -> T = x\n\
        fn apply(f, x) = f(x)\n\
        print(\"${apply(identity, 5)} ${apply(identity, \"hi\")}\")\n");
}

#[test]
fn resume_inside_an_arm_lambda_is_a_type_error() {
    // A lambda body runs when called, not where it is written, so the
    // arm's continuation is not live inside it ([EFFECTS-RESUME]).
    let errs = check(
        "effect E { control op: fn() -> int }\n\
         fn go() -> int !E = perform E.op()\n\
         let r = {\n\
             handle E {\n\
                 op => {\n\
                     let f = |x| => resume(x)\n\
                     f(9)\n\
                 }\n\
             }\n\
             go()\n\
         }\n",
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("not at top level or in a lambda body")),
        "expected the lambda-resume rejection, got: {errs:?}"
    );
}

#[test]
fn assignment_to_undeclared_name_is_an_error() {
    let errs = check("fn main() -> Unit = {\n  neverDeclared = 100\n}\n");
    assert!(errs.iter().any(|e| e
        .message
        .contains("assignment to undeclared `neverDeclared`")));
}

#[test]
fn extern_declarations_register_signatures() {
    // An `extern fn` exercises `collect_extern`, `record_fn_params` over
    // `ExternParameter`, and the published signature used at the call site.
    ok("extern fn c_add(a: int, b: int) -> int\n\
        fn use_it() -> int = c_add(1, 2)\n");
    // An extern with no declared return type defaults to Unit.
    ok("extern fn c_log(msg: string)\n\
        fn use_log() -> Unit = c_log(\"x\")\n");
}

#[test]
fn unimplemented_validated_record_syntax_is_rejected() {
    let errs = check(
        "fn validate(value) = true\n\
         type Item = { value: int } where validate\n",
    );
    assert!(errs.iter().any(|e| e
        .message
        .contains("validated record `where` is not supported")));
}

#[test]
fn infer_program_publishes_functions_and_unions() {
    let parsed = parse_program(
        "type Color = Red | Green\n\
         fn dbl(x: int) -> int = x * 2\n\
         let g = fn(n) => n + 1\n\
         let n = dbl(21)\n",
    );
    let info = infer_program(&parsed.program);
    // Resolved function signatures and union tags are published.
    assert!(info.functions.contains_key("dbl"));
    assert_eq!(
        info.unions.get("Color").map(Vec::len),
        Some(2),
        "Color variants published"
    );
    // The lambda's resolved type is published keyed by source position.
    assert!(!info.lambdas.is_empty(), "lambda type published");
    // The unannotated `let n = dbl(21)` resolves to `int`, published by
    // position. Implements [LSP-HOVER-VARIABLES]
    assert!(
        info.lets.values().any(|t| t.to_string() == "int"),
        "let type published: {:?}",
        info.lets
    );
}

#[test]
fn conflicting_effect_site_keys_are_dropped() {
    // Two sites sharing one (line, column) key with DIFFERENT resolutions
    // (string-interpolation fragments re-parse at fragment-relative
    // positions) must both be dropped; agreeing duplicates are kept.
    // Implements [EFFECTS-GENERIC-INSTANTIATION].
    use crate::info::PerformSite;
    use crate::ty::Type;
    let site = |t: Type| PerformSite {
        op: crate::info::OpType {
            params: Vec::new(),
            ret: t.clone(),
            mode: osprey_ast::OperationMode::default(),
        },
        effect_args: vec![t],
    };
    let entries = vec![
        ((1, 0), site(Type::int())),
        ((1, 0), site(Type::string())),
        ((1, 0), site(Type::int())),
        ((2, 4), site(Type::bool())),
        ((3, 1), site(Type::unit())),
        ((3, 1), site(Type::unit())),
    ];
    let out = crate::check::dedupe_sites(entries.into_iter());
    assert!(!out.contains_key(&(1, 0)), "conflicted key must be dropped");
    assert!(out.contains_key(&(2, 4)));
    assert!(out.contains_key(&(3, 1)), "agreeing duplicates are kept");
}

#[test]
fn single_variant_same_name_type_is_a_record() {
    // `type Foo = Foo` is the single-variant record form (`is_record`); using
    // the bare name is a record value, and `infer_program` publishes it.
    let parsed = parse_program("type Foo = Foo\nlet x = Foo\n");
    let info = infer_program(&parsed.program);
    let foo = info.ctors.get("Foo").expect("Foo ctor published");
    assert!(foo.owner_is_record);
}
