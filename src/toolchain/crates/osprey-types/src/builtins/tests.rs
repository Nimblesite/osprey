use super::*;

#[test]
fn scalar_externs_remain_provable_through_deferred_methods() {
    for name in ["print", "read"] {
        crate::effect_rows_tests::assert_accepted(&format!(
            "extern fn {name}(value: int) -> int\n\
             fn through(value) = value.{name}()\nlet answer = through(41)\n"
        ));
    }
}

#[test]
fn registry_classifies_the_compilers_ambient_builtin_inventory() {
    let mut ambient = TypeEnv::new();
    for declare in [
        host::files,
        host::terminal,
        host::http,
        host::websocket,
        tensors,
    ] {
        declare(&mut ambient);
    }
    // The registry lives in Nimblesite/osprey-packages, laid from its main by `make
    // downstream`; read at run time so the crate builds without it. [REPO-SPLIT-CANARY]
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../osprey/packages/manager/core/src/surface.osp"
    );
    let surface = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("{path}: {error}; lay the package manager with `make downstream`"),
    };
    for name in ambient.bound_names() {
        assert!(
            name.starts_with("http")
                || name.starts_with("websocket")
                || surface.contains(&format!("named(\"{name}\",")),
            "registry leaves compiler ambient builtin {name} unclassified"
        );
    }
}

/// Assert each builtin `name` prints exactly `signature`.
fn assert_signatures(expected: &[(&str, &str)]) {
    for &(name, signature) in expected {
        assert_eq!(
            builtin_signature(name).as_deref(),
            Some(signature),
            "{name}"
        );
    }
}

#[test]
fn registers_core_and_polymorphic_builtins() {
    let e = base_env();
    assert!(e.get("print").is_some());
    assert_eq!(e.get("map").unwrap().vars.len(), 2);
    assert_eq!(e.get("await").unwrap().vars.len(), 1);
    assert_eq!(
        builtin_signature("fiberDone").as_deref(),
        Some("fiberDone : (Fiber<t0>) -> int")
    );
    assert_eq!(
        builtin_signature("abs").as_deref(),
        Some("abs : (t0) -> t0")
    );
    assert_eq!(
        builtin_signature("intDiv").as_deref(),
        Some("intDiv : (t0, t0) -> t0")
    );
    assert_eq!(
        builtin_signature("wrapAdd").as_deref(),
        Some("wrapAdd : (t0, t0) -> t0")
    );
}

#[test]
fn process_builtins_match_the_result_returning_runtime() {
    assert_eq!(
        builtin_signature("spawnProcess").as_deref(),
        Some("spawnProcess : (string, (int, int, string) -> Unit) -> Result<int, string>")
    );
    assert_eq!(
        builtin_signature("awaitProcess").as_deref(),
        Some("awaitProcess : (int) -> int")
    );
    assert_eq!(
        builtin_signature("cleanupProcess").as_deref(),
        Some("cleanupProcess : (int) -> Unit")
    );
}

#[test]
fn file_builtins_include_the_host_tensor_loader() {
    // [BUILTIN-FILE-TENSOR] A safetensors tensor loads as a plain host list,
    // never as a GpuBuffer, so a host interpretation needs no GPU surface.
    // [LLM-WEIGHTS-MAPPED] The program's use of a mapped file picks the
    // element. [BUILTIN-FILE-BYTES] Binary stays bytes; a string would stop
    // at a NUL. [BUILTIN-FILE-SIZE] A reader asks the length first.
    assert_signatures(&[
        (
            "loadTensor",
            "loadTensor : (string, string) -> Result<List<float>, string>",
        ),
        (
            "gpuMapFile",
            "gpuMapFile : (string, int, int) -> Result<GpuBuffer<t0>, string>",
        ),
        (
            "readFileBytes",
            "readFileBytes : (string, int, int) -> Result<List<int>, string>",
        ),
        ("fileSize", "fileSize : (string) -> Result<int, string>"),
    ]);
}

/// [GPU-BUFFER-TENSOR] Reject unsupported requested storage before codegen.
#[test]
fn tensor_loader_result_obligations_survive_aliases_and_helpers() {
    for (prefix, callee) in [
        ("", "gpuLoadTensor"),
        ("let load = gpuLoadTensor\n", "load"),
        ("fn load(path, name) = gpuLoadTensor(path, name)\n", "load"),
    ] {
        for name in ["float", "float32", "float16"].into_iter().chain(
            crate::numeric::Numeric::ALL
                .into_iter()
                .filter(|n| !n.is_float())
                .map(crate::numeric::Numeric::name)
                .chain(["bool", "string"]),
        ) {
            assert_tensor_request(prefix, callee, name);
        }
    }
}

fn assert_tensor_request(prefix: &str, callee: &str, name: &str) {
    let source = format!(
        "{prefix}fn wanted() -> Result<GpuBuffer<{name}>, string> = {callee}(\"x\", \"w\")\n"
    );
    let errors = crate::testutil::check(&source);
    if matches!(name, "float" | "float32" | "float16") {
        assert!(errors.is_empty(), "{source}: {errors:?}");
    } else {
        assert!(
            !errors.is_empty(),
            "unsupported tensor element accepted: {source}"
        );
    }
}

/// [GPU-MATMUL] Function values retain the float/float32 operand restriction.
#[test]
fn matrix_operand_obligations_survive_aliases_and_helpers() {
    for (prefix, callee) in [
        ("", "gpuMatMul"),
        ("let product = gpuMatMul\n", "product"),
        (
            "fn product(a, b, r, k, c) = gpuMatMul(a, b, r, k, c)\n",
            "product",
        ),
    ] {
        for name in ["float", "float32", "float16", "int", "uint8", "bool"] {
            assert_matrix_request(prefix, callee, name);
        }
    }
}

fn assert_matrix_request(prefix: &str, callee: &str, name: &str) {
    let source = format!(
        "{prefix}fn wanted(a: GpuBuffer<{name}>, b: GpuBuffer<{name}>) = {callee}(a, b, 1, 1, 1)\n"
    );
    let errors = crate::testutil::check(&source);
    assert_eq!(
        errors.is_empty(),
        matches!(name, "float" | "float32"),
        "{source}: {errors:?}"
    );
}

/// [GPU-MATMUL] A better constraint diagnostic keeps the original call site.
#[test]
fn matrix_operand_rejection_keeps_its_source_position() {
    let errors = crate::testutil::check(include_str!(
        "../../../../../osprey/examples/failscompilation/gpu_matmul_int_buffer.ospo"
    ));
    assert!(
        errors.iter().any(|error| error
            .position
            .is_some_and(|at| at.line == 4 && at.column == 4)),
        "{errors:?}"
    );
}

#[test]
fn network_builtins_match_the_runtime_status_abi() {
    let expected = [
        (
            "writeFile",
            "writeFile : (string, string) -> Result<int, string>",
        ),
        (
            "httpCloseClient",
            "httpCloseClient : (int) -> Result<int, string>",
        ),
        (
            "httpGet",
            "httpGet : (int, string, string) -> Result<int, string>",
        ),
        (
            "httpResponseFree",
            "httpResponseFree : (int) -> Result<int, string>",
        ),
        (
            "httpPost",
            "httpPost : (int, string, string, string) -> Result<int, string>",
        ),
        (
            "httpPostResponse",
            "httpPostResponse : (int, string, string, string) -> Result<int, string>",
        ),
        (
            "httpPut",
            "httpPut : (int, string, string, string) -> Result<int, string>",
        ),
        (
            "httpDelete",
            "httpDelete : (int, string, string) -> Result<int, string>",
        ),
        (
            "httpListen",
            "httpListen : (int, (string, string, string, string) -> HttpResponse) -> Result<int, string>",
        ),
        (
            "httpStopServer",
            "httpStopServer : (int) -> Result<int, string>",
        ),
        ("httpClientGone", "httpClientGone : () -> int"),
        (
            "httpKeepAlive",
            "httpKeepAlive : (int) -> Result<int, string>",
        ),
        ("websocketClose", "websocketClose : (int) -> int"),
        ("jsonFree", "jsonFree : (int) -> Result<int, string>"),
    ];
    assert_signatures(&expected);
}

#[test]
fn public_maps_use_the_runtime_string_key_abi() {
    let expected = [
        ("Map", "Map : () -> Map<string, t0>"),
        (
            "mapSet",
            "mapSet : (Map<string, t0>, string, t0) -> Map<string, t0>",
        ),
        (
            "mapGet",
            "mapGet : (Map<string, t0>, string) -> Result<t0, string>",
        ),
        ("mapKeys", "mapKeys : (Map<string, t0>) -> List<string>"),
    ];
    assert_signatures(&expected);
}

#[test]
fn iterator_builtins_do_not_advertise_runtime_lists() {
    let expected = [
        ("range", "range : (int, int) -> Iterator<int>"),
        ("map", "map : (Iterator<t0>, (t0) -> t1) -> Iterator<t1>"),
        (
            "filter",
            "filter : (Iterator<t0>, (t0) -> bool) -> Iterator<t0>",
        ),
        ("forEach", "forEach : (Iterator<t0>, (t0) -> Unit) -> Unit"),
        ("fold", "fold : (Iterator<t0>, t1, (t1, t0) -> t1) -> t1"),
    ];
    assert_signatures(&expected);
}

#[test]
fn list_combinators_and_sort_match_the_runtime_list_surface() {
    // [BUILTIN-LIST-MAP], [BUILTIN-LIST-FILTER], [BUILTIN-LIST-FOLD],
    // [BUILTIN-LIST-SORT].
    let expected = [
        ("mapList", "mapList : (List<t0>, (t0) -> t1) -> List<t1>"),
        (
            "filterList",
            "filterList : (List<t0>, (t0) -> bool) -> List<t0>",
        ),
        (
            "foldList",
            "foldList : (List<t0>, t1, (t1, t0) -> t1) -> t1",
        ),
        (
            "listSort",
            "listSort : (List<int> | List<string>) -> List<t0>",
        ),
    ];
    assert_signatures(&expected);
}

#[test]
fn foreign_callable_bindings_do_not_inherit_builtin_argument_constraints() {
    for name in ["print", "toString", "length"] {
        for call in [
            "foreign(callback)",
            "foreign(callback: callback)",
            "callback |> foreign",
            "callback.foreign()",
            "through(callback)",
        ] {
            let source = format!(
                "extern fn {name}(callback: fn() -> int) -> int\n\
                 fn through(callback) = callback.{name}()\n\
                 let callback = fn() => 41\nlet answer = {}\n",
                call.replace("foreign", name)
            );
            crate::effect_rows_tests::assert_accepted(&source);
        }
    }
}

/// Built-in constraints judge every shape a call can hand them: a record, a
/// list, a function inside a union, an open variable, a non-buffer
/// [BUILTIN-LIST-SORT] [GPU-MATMUL] [GPU-BUFFER-ELEM] [BUILTIN-TOSTRING].
#[test]
fn builtin_constraints_judge_every_argument_shape() {
    let decls = "type P = { x: int }\ntype Holder = Empty | Has { f: fn(int) -> int }\ntype Shape = Dot | Circle { r: int }\n";
    let cases = [
        ("let s = listSort([P { x: 1 }])", false),
        ("let s = listSort([[1]])", false),
        ("let s = listSort([\"b\", \"a\"])", true),
        ("fn product(a, b) = gpuMatMul(a, b, 1, 1, 1)", true),
        ("let m = gpuMatMul(1, 2, 1, 1, 1)", false),
        ("fn size(b: GpuBuffer<P>) = gpuLength(b)", false),
        ("fn size(b: GpuBuffer<bool>) = gpuLength(b)", true),
        ("let shown = print(Empty)", false),
        ("let shown = print(Circle { r: 2 })", true),
        ("let shown = toString(Has { f: fn(n) => n })", false),
    ];
    for (body, accepted) in cases {
        let source = format!("{decls}{body}\n");
        let errors = crate::testutil::check(&source);
        assert_eq!(errors.is_empty(), accepted, "{source}: {errors:?}");
    }
}

/// The diagnostic names the path from the offense out to the operand,
/// innermost first, at any depth [BUILTIN-STRUCTURAL-EQUALS].
#[test]
fn structural_equals_names_the_path_to_an_undecided_part() {
    let decls = "type Holder = Empty | Has { step: fn(int) -> int }\ntype Nest = Leaf | Node { inner: Holder }\ntype Bag = { items: List<Nest> }\n";
    let cases = [
        ("structuralEquals(Empty, Empty)", "`Holder` with `structuralEquals`: field `step` inside variant `Has` is"),
        ("structuralEquals(Leaf, Leaf)", "`Nest` with `structuralEquals`: field `step` inside variant `Has` inside field `inner` inside variant `Node` is"),
        ("structuralEquals(Bag { items: [] }, Bag { items: [Leaf] })", "field `step` inside variant `Has` inside field `inner` inside variant `Node` inside list element inside field `items` is"),
    ];
    for (call, needle) in cases {
        let source = format!("{decls}let same = {call}\n");
        let errors = crate::testutil::check(&source);
        assert!(
            errors.iter().any(|e| e.message.contains(needle)),
            "{source}: {errors:?}"
        );
    }
    let printable = format!("{decls}let shown = toString(Leaf)\n");
    assert!(
        !crate::testutil::check(&printable).is_empty(),
        "{printable}"
    );
}

/// Callback and signature lookups answer only for what a built-in declares.
#[test]
fn callback_and_signature_lookups_refuse_what_is_not_declared() {
    assert_eq!(builtin_callback_type("httpListen", 0), None);
    assert_eq!(builtin_callback_type("httpListen", 9), None);
    assert_eq!(builtin_callback_type("notABuiltin", 0), None);
    assert!(builtin_callback_type("mapList", 1).is_some());
    let matmul = builtin_signature("gpuMatMul").unwrap_or_default();
    assert!(matmul.starts_with("gpuMatMul : ("), "{matmul}");
    assert!(builtin_signature("toString").is_some_and(|s| s.contains("printable")));
    assert_eq!(builtin_signature("notABuiltin"), None);
}
