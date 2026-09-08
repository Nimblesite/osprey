//! Target restrictions fail before LLVM or an external toolchain is invoked.
use std::path::Path;
use std::process::{Command, Output};

fn compile(source: &str, target: &str) -> std::io::Result<Output> {
    let directory = std::env::temp_dir().join(format!(
        "osprey_target_caps_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory)?;
    let input = directory.join("check.osp");
    std::fs::write(&input, source)?;
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args([input.as_os_str(), "--llvm".as_ref()])
        .arg(format!("--target={target}"))
        .output();
    std::fs::remove_dir_all(directory)?;
    output
}

fn rejects(source: &str, target: &str, feature: &str) -> std::io::Result<()> {
    let output = compile(source, target)?;
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{target} accepted {feature}");
    assert!(error.contains(target) && error.contains(feature), "{error}");
    assert!(error.contains("line"), "missing source location: {error}");
    assert!(output.stdout.is_empty(), "rejected program emitted LLVM IR");
    Ok(())
}

#[test]
fn resumable_effects_reject_for_both_flavors_before_ir() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for extension in ["osp", "ospml"] {
        let path = root.join(format!(
            "tests/effects/resume/resume_value_rewrite.test.{extension}"
        ));
        for target in ["wasm32", "ios", "ios-sim", "android-arm64", "android-x64"] {
            let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
                .arg(&path)
                .arg("--llvm")
                .arg(format!("--target={target}"))
                .output()?;
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "accepted {target} {extension}");
            // [MULTI-WASM] The rejection names the OPERATION whose request
            // cannot be suspended, not the `resume` keyword: a row that cannot
            // say which effects will start working when stack switching lands
            // is a row that cannot be planned against.
            assert!(
                error.contains("a continuation for `Supply.next`"),
                "{error}"
            );
            assert!(output.stdout.is_empty());
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
    let source = "effect Supply { next: fn() -> int }\n\
                  fn ask() = handle Supply next => resume(1) in perform Supply.next()\n";
    for (target, expected, forbidden) in [
        ("wasm32", "stack-switching", "synchronous host call"),
        ("ios", "synchronous host call", "stack-switching"),
        ("ios-sim", "synchronous host call", "stack-switching"),
        ("android-arm64", "synchronous host call", "stack-switching"),
    ] {
        let output = compile(source, target)?;
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{target} accepted a continuation");
        assert!(
            error.contains("a continuation for `Supply.next`"),
            "{target}: {error}"
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
    for target in ["wasm32", "ios", "ios-sim", "android-arm64", "android-x64"] {
        for name in ["httpGet", "websocketConnect", "spawnProcess"] {
            rejects(&format!("let aliased = {name}\n"), target, name)?;
        }
    }
    Ok(())
}

#[test]
fn wasm_rejects_fibers_terminals_and_foreign_imports() -> std::io::Result<()> {
    for source in [
        "let task = spawn 1\n",
        "let task = Channel(1)\n",
        "sleep(1)\n",
    ] {
        rejects(source, "wasm32", "fiber")?;
    }
    rejects("let columns = termCols()\n", "wasm32", "terminal")?;
    rejects("extern fn absent(value: int) -> int\n", "wasm32", "absent")
}

#[test]
fn wasm_preserves_browser_imports_and_substituting_effects() -> std::io::Result<()> {
    let source = "extern fn osprey_web_render(html: string) -> int\n\
                  extern fn osprey_web_command(payload: string) -> int\n\
                  effect Supply { ask: fn() -> int }\n\
                  let answer = handle Supply ask => 41 in perform Supply.ask()\n\
                  print(answer)\n";
    let output = compile(source, "wasm32")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("define i32 @main"));
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
        let output = compile(source, "wasm32")?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
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
        let output = compile(source, target)?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
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
                    let result = handle Supply ask => 41 in osprey_web_dispatch(\"boot\")\n\
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
            let output = compile(source, target)?;
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success());
            assert!(error.contains(feature), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
    Ok(())
}
