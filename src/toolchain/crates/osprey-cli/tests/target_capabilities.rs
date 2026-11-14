//! Target restrictions fail before LLVM or an external toolchain is invoked.
use std::path::Path;
use std::process::{Command, Output};

/// Every target whose host or runtime cannot provide the full native surface.
/// The Mac slice is held to the iPhone rules so one program serves both.
/// [IOS-TARGET-MACOS]
const RESTRICTED_TARGETS: [&str; 6] = [
    "wasm32",
    "ios",
    "ios-sim",
    "macos",
    "android-arm64",
    "android-x64",
];

/// Runs the compiler on an existing program file for `target`.
fn compile_file(path: &Path, target: &str) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args([path.as_os_str(), "--llvm".as_ref()])
        .arg(format!("--target={target}"))
        .output()
}

fn compile(source: &str, target: &str) -> std::io::Result<Output> {
    let directory = std::env::temp_dir().join(format!(
        "osprey_target_caps_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory)?;
    let input = directory.join("check.osp");
    std::fs::write(&input, source)?;
    let output = compile_file(&input, target);
    std::fs::remove_dir_all(directory)?;
    output
}

/// Compiles `source` for `target`, asserting success and returning its stdout.
fn accepts(source: &str, target: &str) -> std::io::Result<String> {
    let output = compile(source, target)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Asserts the compile failed on `feature` with no IR emitted; returns stderr.
fn refused(output: &Output, feature: &str) -> String {
    let error = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!output.status.success(), "accepted `{feature}`");
    assert!(error.contains(feature), "{error}");
    assert!(output.stdout.is_empty(), "rejected program emitted LLVM IR");
    error
}

fn rejects(source: &str, target: &str, feature: &str) -> std::io::Result<()> {
    let error = refused(&compile(source, target)?, feature);
    assert!(error.contains(target), "{error}");
    assert!(error.contains("line"), "missing source location: {error}");
    Ok(())
}

#[test]
fn declared_control_requires_support_without_a_resume_expression() -> std::io::Result<()> {
    let source = "effect Supply { value: fn() -> int control stop: fn() -> int }\n\
                  let answer = {\n\
                      handle Supply {\n\
                          value => 41 stop => 0\n\
                      }\n\
                      perform Supply.stop()\n\
                  }\n\
                  print(answer)\n";
    for target in RESTRICTED_TARGETS {
        rejects(source, target, "a continuation for `Supply.stop`")?;
    }
    Ok(())
}

#[test]
fn resumable_effects_reject_for_both_flavors_before_ir() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    for extension in ["osp", "ospml"] {
        let path = root.join(format!(
            "src/osprey/tests/effects/resume/resume_value_rewrite.test.{extension}"
        ));
        for target in RESTRICTED_TARGETS {
            // [MULTI-WASM] The rejection names the OPERATION whose request
            // cannot be suspended, not the `resume` keyword: a row that cannot
            // say which effects will start working when stack switching lands
            // is a row that cannot be planned against.
            let _ = refused(
                &compile_file(&path, target)?,
                "a continuation for `Supply.next`",
            );
        }
    }
    Ok(())
}

/// A rejection must explain why THIS target cannot run the program. The
/// resumable-effect diagnostic once told every target about a WebAssembly
/// proposal, so an iPhone build was answered with a roadmap for a runtime it
/// does not use. [IOS-TARGET-CAPABILITIES] [WASM-TARGET-CAPABILITIES]
#[test]
fn a_continuation_rejection_explains_the_target_it_names() -> std::io::Result<()> {
    let source = "effect Supply { control next: fn() -> int }\n\
                  fn ask() = {\n\
                      handle Supply {\n\
                          next => resume(1)\n\
                      }\n\
                      perform Supply.next()\n\
                  }\n";
    for (target, expected, forbidden) in [
        ("wasm32", "stack-switching", "synchronous host call"),
        ("ios", "synchronous host call", "stack-switching"),
        ("ios-sim", "synchronous host call", "stack-switching"),
        ("macos", "synchronous host call", "stack-switching"),
        ("android-arm64", "synchronous host call", "stack-switching"),
    ] {
        let error = refused(
            &compile(source, target)?,
            "a continuation for `Supply.next`",
        );
        assert!(
            error.contains(expected),
            "{target} must explain itself: {error}"
        );
        assert!(
            !error.contains(forbidden),
            "{target} must not cite another target's roadmap: {error}"
        );
    }
    Ok(())
}

#[test]
fn unsupported_runtime_builtins_reject_through_aliases() -> std::io::Result<()> {
    for target in RESTRICTED_TARGETS {
        for name in ["httpGet", "websocketConnect", "spawnProcess"] {
            rejects(&format!("let aliased = {name}\n"), target, name)?;
        }
    }
    Ok(())
}

#[test]
fn wasm_rejects_fibers_terminals_and_foreign_imports() -> std::io::Result<()> {
    for (source, feature) in [
        ("let task = spawn 1\n", "fiber"),
        ("let task = Channel(1)\n", "fiber"),
        ("sleep(1)\n", "fiber"),
        ("let columns = termCols()\n", "terminal"),
        ("extern fn absent(value: int) -> int\n", "absent"),
    ] {
        rejects(source, "wasm32", feature)?;
    }
    Ok(())
}

/// Each WASI libc symbol must be declared once and called once through the C
/// ABI, so a second declaration or a missing call cannot satisfy the contract.
fn assert_wasi_void_calls(ir: &str) {
    for expected in [
        "declare void @perror(i8*)",
        "declare void @abort()",
        "call void @perror(i8*",
        "call void @abort()",
    ] {
        assert_eq!(
            ir.matches(expected).count(),
            1,
            "expected exactly one `{expected}`:\n{ir}"
        );
    }
}

/// Both first-class imports get one forwarder at the Osprey function-value ABI.
fn assert_wasi_closures(ir: &str) {
    for wrapper in [
        "define internal i64 @__fnval_perror(",
        "define internal i64 @__fnval_abort(",
    ] {
        assert_eq!(
            ir.matches(wrapper).count(),
            1,
            "expected exactly one `{wrapper}`:\n{ir}"
        );
    }
}

/// WASI libc supplies `void perror(const char*)` and `void abort(void)`, so a
/// wasm32 program may import both and the emitted module must carry exactly one
/// declaration and one call per symbol. [WASI-LIBC-ABI]
#[test]
fn wasm_accepts_wasi_libc_perror_and_abort() -> std::io::Result<()> {
    let source = "extern fn perror(message: string) -> Unit\nextern fn abort() -> Unit\n\
                  perror(\"wasm libc\")\nabort()\n";
    assert_wasi_void_calls(&accepts(source, "wasm32")?);
    Ok(())
}

/// The wasm runtime carries the shared frame writers (`native_scene.c`), so a
/// wasm32 build of any `@osprey/native` application calls them: the VS Code
/// host compiles the package editor and the Workbench this way. They are typed
/// builtins with the C signatures fixed by the compiler, never raw externs, so
/// the registry publishes `@osprey/native` [PACKAGE-CAPABILITY-CONFINEMENT].
/// Each writer is declared once and called. [NATIVE-SHM-VIEW] [NATIVE-SHM-SCENE] [NATIVE-SHM-ASK]
#[test]
fn wasm_accepts_the_shared_frame_writers_its_runtime_carries() -> std::io::Result<()> {
    let source = "fn main() = print(toString(nativeSceneShape(0, 0, 1.0, 2.0, 3.0, 4.0, 0.0, 255, 0, \"t\")) + toString(nativeViewExtend(nativeViewRow(2, -1, -1, \"hi\", 255, 0, 12.0, 0.0, 0.0, 0.0, 400), 10.0, 0.0, 0.0, 0, 0, \"\")) + toString(nativeAsk(1, 2, \"ask\")))\n";
    let ir = accepts(source, "wasm32")?;
    for writer in [
        "osp_native_scene_shape",
        "osp_native_view_row",
        "osp_native_view_extend",
        "osp_native_ask",
    ] {
        assert_eq!(
            ir.lines().filter(|line| line.starts_with("declare ") && line.contains(&format!("@{writer}("))).count(),
            1,
            "{writer} must be declared once:\n{ir}"
        );
        assert!(
            ir.contains(&format!("@{writer}(")),
            "{writer} is never called"
        );
    }
    Ok(())
}

/// A first-class import goes through the named function's environment-dropping
/// forwarder: the wrapper returns the `i64` Unit zero, only the wrapped C callee
/// is `void`, and the forwarder body makes the single call. [WASI-LIBC-ABI]
#[test]
fn wasm_wraps_wasi_libc_imports_as_first_class_values() -> std::io::Result<()> {
    let source = "extern fn perror(message: string) -> Unit\n\
                  extern fn abort() -> Unit\n\
                  fn main() =\n\
                      print(\"${toString(listLength([perror]))}${toString(listLength([abort]))}\")\n";
    let ir = accepts(source, "wasm32")?;
    assert_wasi_closures(&ir);
    assert_wasi_void_calls(&ir);
    Ok(())
}

/// The WASI libc ABI is a fixed C signature per symbol, so a same-named import
/// with a different arity or type is refused before any IR exists rather than
/// called through a mismatched prototype. [WASI-LIBC-ABI]
#[test]
fn wasm_rejects_wrong_wasi_libc_signatures() -> std::io::Result<()> {
    for source in [
        "extern fn perror(value: int) -> Unit\nprint(41)\n",
        "extern fn perror(message: string) -> int\nprint(perror(\"x\"))\n",
        "extern fn abort(code: int) -> Unit\nprint(41)\n",
        "extern fn abort() -> int\nprint(abort())\n",
    ] {
        rejects(source, "wasm32", "WASI libc ABI")?;
    }
    Ok(())
}

#[test]
fn wasm_preserves_browser_imports_and_substituting_effects() -> std::io::Result<()> {
    let source = "extern fn osprey_web_render(html: string) -> int\n\
                  extern fn osprey_web_command(payload: string) -> int\n\
                  effect Supply { ask: fn() -> int }\n\
                  let answer = {\n\
                      handle Supply {\n\
                          ask => 41\n\
                      }\n\
                      perform Supply.ask()\n\
                  }\n\
                  print(answer)\n";
    assert!(accepts(source, "wasm32")?.contains("define i32 @main"));
    Ok(())
}

#[test]
fn builtin_spellings_in_local_bindings_are_not_platform_capabilities() -> std::io::Result<()> {
    for source in [
        "let sleep = 41\nprint(sleep)\n",
        "fn identity(sleep) = sleep\nprint(identity(41))\n",
        "let identity = fn(httpGet) => httpGet\nprint(identity(41))\n",
        "let value = match 41 { sleep => sleep }\nprint(value)\n",
    ] {
        let _ = accepts(source, "wasm32")?;
    }
    Ok(())
}

#[test]
fn namespaced_functions_and_scalar_ios_host_imports_are_preserved() -> std::io::Result<()> {
    for (source, target) in [
        (
            "namespace App { fn httpGet(x) = x }\nprint(App::httpGet(41))\n",
            "wasm32",
        ),
        (
            "extern fn httpGet(value: int) -> int\nprint(httpGet(41))\n",
            "ios-sim",
        ),
    ] {
        let _ = accepts(source, target)?;
    }
    Ok(())
}

/// An archive carries no library, so an app-library target refuses a link
/// directive instead of dropping it; a directive scoped to a native operating
/// system does not apply there. [FFI-LINK-DIRECTIVES] [IOS-TARGET-CAPABILITIES]
#[test]
fn app_library_targets_refuse_unscoped_link_directives() -> std::io::Result<()> {
    let linked = "// @link: sqlite3\nextern fn sqlite3_libversion_number() -> int\nprint(sqlite3_libversion_number())\n";
    let scoped = "// @link(windows): ospgfx\nextern fn host_ready() -> int\nprint(host_ready())\n";
    for target in ["ios", "ios-sim", "macos", "android-arm64", "android-x64"] {
        let _ = refused(
            &compile(linked, target)?,
            &format!("target `{target}` does not support link directive `// @link: sqlite3`"),
        );
        let _ = accepts(scoped, target)?;
    }
    Ok(())
}

/// The Python host exchanges int, float, bool and string; a handle it cannot
/// represent fails at compile time instead of in `python -m osprey`.
#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn python_rejects_pointer_exports() -> std::io::Result<()> {
    let _ = refused(
        &compile("fn keep(window: Ptr) = window\n", "python")?,
        "Python export `keep` passes a Ptr",
    );
    Ok(())
}

#[test]
fn wasm_browser_boundary_rejects_wrong_or_unresolved_signatures() -> std::io::Result<()> {
    for source in [
        "extern fn osprey_web_render(value: int) -> float\n",
        "extern fn osprey_web_command(value: string, extra: int) -> int\n",
        "fn osprey_web_dispatch(value) = value\n",
        "fn osprey_web_dispatch(value: string) = true\n",
        "namespace App { fn osprey_web_dispatch(value: int) = value }\n",
    ] {
        rejects(source, "wasm32", "browser ABI")?;
    }
    Ok(())
}

#[test]
fn browser_dispatcher_requires_its_own_effect_handlers() -> std::io::Result<()> {
    let source = "effect Supply { ask: fn() -> int }\n\
                  fn osprey_web_dispatch(message: string) = perform Supply.ask()\n\
                  fn main() = {\n\
                  let result = {\n\
                      handle Supply {\n\
                          ask => 41\n\
                      }\n\
                      osprey_web_dispatch(\"boot\")\n\
                  }\n\
                  print(result)\n\
                  }\n";
    rejects(source, "wasm32", "Supply.ask")
}

#[test]
fn android_library_abi_rejects_unhandled_exports_and_aggregate_imports() -> std::io::Result<()> {
    for target in ["android-arm64", "android-x64"] {
        for (source, feature) in [
            ("effect Supply { ask: fn() -> int }\nfn answer() = perform Supply.ask()\nfn main() = print(0)\n", "Supply.ask"),
            ("extern fn host(values: List<int>) -> int\n", "unsupported C ABI signature"),
        ] {
            let _ = refused(&compile(source, target)?, feature);
        }
    }
    Ok(())
}
