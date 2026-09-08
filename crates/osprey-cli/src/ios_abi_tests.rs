use super::*;
use std::path::{Path, PathBuf};

const SOURCE: &str = "extern fn host_log(message: string) -> int\n\
    fn greet(name: string) -> string = \"hello ${name}\"\n\
    fn flag(b: bool) -> bool = !b\n\
    fn shout(s: string) = print(s)\n\
    fn scaled(n: int) -> float = toFloat(n) * 1.5\n\
    fn twice(x) = x\n\
    fn total(a: int, b: int) -> int = a + b ?: 0\n\
    fn count(xs: List<int>) -> int = listLength(xs)\n\
    fn main() = print(\"${host_log(greet(\\\"x\\\"))}\")\n";

fn checked(source: &str) -> (Program, ProgramTypes, String) {
    let parsed = osprey_syntax::parse_program(source);
    assert!(
        parsed.errors.is_empty(),
        "fixture parses: {:?}",
        parsed.errors
    );
    let program = parsed.program;
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "fixture types: {errors:?}");
    let types = osprey_types::infer_program(&program);
    let ir = osprey_codegen::compile_program(&program).expect("fixture lowers");
    (program, types, ir)
}

fn abi_of(source: &str) -> (HostAbi, String) {
    let (program, types, ir) = checked(source);
    (host_abi(&program, &types, &ir).expect("abi"), ir)
}

#[test]
fn exports_every_defined_scalar_function_and_nothing_else() {
    // [IOS-HOST-ABI]
    let (abi, _) = abi_of(SOURCE);
    let names: Vec<&str> = abi.exports.iter().map(|e| e.c_name.as_str()).collect();
    assert_eq!(
        names,
        [
            "osprey_greet",
            "osprey_flag",
            "osprey_shout",
            "osprey_scaled",
            "osprey_total"
        ]
    );
    // `twice` is generic (inlined, no symbol), `count` takes a List, and
    // `main` is the entry: none of them is an export.
    assert!(abi.exports.iter().all(|e| e.symbol == e.source_name));
    let shout = abi
        .exports
        .iter()
        .find(|e| e.symbol == "shout")
        .expect("shout");
    assert_eq!(shout.ret, CType::Unit);
    assert_eq!(shout.params, vec![("s".to_string(), CType::Str)]);
    let imports: Vec<&str> = abi.imports.iter().map(|i| i.symbol.as_str()).collect();
    assert_eq!(imports, ["host_log"]);
}

#[test]
fn thunks_rename_the_entry_and_forward_with_the_c_bool_convention() {
    // [IOS-TARGET-ENTRY] [IOS-HOST-ABI]
    let (abi, ir) = abi_of(SOURCE);
    let out = with_host_abi(&ir, &abi).expect("thunked");
    assert!(out.contains("define i32 @osprey_main() "), "entry renamed");
    assert!(!out.contains("@main("), "no `main` symbol survives");
    assert!(out.contains("define zeroext i1 @osprey_flag(i1 zeroext %p0) {"));
    assert!(out.contains("  %r = call i1 @flag(i1 %p0)\n  ret i1 %r"));
    assert!(out
        .contains("define void @osprey_shout(i8* %p0) {\n  call i64 @shout(i8* %p0)\n  ret void"));
    assert!(out.contains("define i8* @osprey_greet(i8* %p0) {"));
    assert!(out.contains("define double @osprey_scaled(i64 %p0) {"));
    assert!(out.contains("define i64 @osprey_total(i64 %p0, i64 %p1) {"));
    assert!(with_host_abi("; no entry here", &abi).is_err());
}

#[test]
fn header_declares_entry_exports_and_imports_in_c() {
    // [IOS-HOST-ABI]
    let (abi, _) = abi_of(SOURCE);
    let h = header_for_target(&abi, "demo.osp", "ios");
    for line in [
        "#pragma once",
        "int32_t osprey_main(void);",
        "// fn greet(name: string) -> string",
        "const char *osprey_greet(const char *name);",
        "bool osprey_flag(bool b);",
        "void osprey_shout(const char *s);",
        "double osprey_scaled(int64_t n);",
        "int64_t osprey_total(int64_t a, int64_t b);",
        "// extern fn host_log(message: string) -> int",
        "int64_t host_log(const char *message);",
        "extern \"C\" {",
    ] {
        assert!(h.contains(line), "header lacks {line:?}:\n{h}");
    }
}

#[test]
fn project_symbols_export_under_their_qualified_c_name() {
    // [IOS-HOST-ABI]
    let source = "namespace app;\nfn greet(name: string) -> string = name\nfn main() = print(greet(\"x\"))\n";
    let program = osprey_project::assemble(
        &osprey_project::ProjectConfig::for_root(Path::new("/tmp/ios-abi-project")),
        &[osprey_project::SourceFile {
            path: PathBuf::from("main.osp"),
            flavor: osprey_syntax::Flavor::Default,
            source: source.to_string(),
            program: osprey_syntax::parse_program(source).program,
        }],
    )
    .expect("assemble")
    .program;
    let types = osprey_types::infer_program(&program);
    let ir = osprey_codegen::compile_program(&program).expect("lowers");
    let abi = host_abi(&program, &types, &ir).expect("abi");
    let greet = abi
        .exports
        .iter()
        .find(|e| e.source_name == "app::greet")
        .expect("greet");
    assert_eq!(greet.c_name, "osprey_app_greet");
    assert!(
        greet.symbol.starts_with("__osp_"),
        "forwards to the mangled symbol"
    );
    assert!(with_host_abi(&ir, &abi)
        .expect("thunked")
        .contains(&format!("call i8* @{}(i8* %p0)", greet.symbol)));
}

#[test]
fn a_boundary_name_clash_is_refused() {
    // [IOS-HOST-ABI]
    let source = "extern fn osprey_greet(s: string) -> int\nfn greet(s: string) -> int = osprey_greet(s)\nfn main() = print(\"${greet(\\\"x\\\")}\")\n";
    let (program, types, ir) = checked(source);
    let err = host_abi(&program, &types, &ir).expect_err("clash");
    assert!(err.contains("osprey_greet"), "{err}");
}

#[test]
fn host_imports_cannot_replace_incompatible_runtime_declarations() {
    // [IOS-HOST-ABI] [ANDROID-HOST-ABI] These used to pass --check and
    // produce an invalid adapter instead of rejecting the runtime collision.
    for (source, symbol) in [
        (
            "extern fn osp_alloc(size: int) -> int\nfn allocate(size: int) = osp_alloc(size)\nfn greeting() = \"hello\" + \" world\"\n",
            "osp_alloc",
        ),
        ("extern fn osp_mem_boot() -> Unit\n", "osp_mem_boot"),
        ("extern fn osp_prof_boot() -> int\n", "osp_prof_boot"),
    ] {
        let (program, types, ir) = checked(source);
        let error = host_abi(&program, &types, &ir).expect_err("runtime import collision");
        assert!(error.contains(symbol) && error.contains("runtime"), "{error}");
    }
}

#[test]
fn c_types_map_only_scalars() {
    let con = |name: &str| Type::Con {
        name: name.to_string(),
        args: vec![],
    };
    assert_eq!(CType::from_type(&con("int")), Some(CType::Int));
    assert_eq!(CType::from_type(&con("Unit")), Some(CType::Unit));
    assert_eq!(CType::from_type(&con("Point")), None);
    assert_eq!(
        CType::from_type(&Type::Con {
            name: "List".to_string(),
            args: vec![con("int")]
        }),
        None
    );
    assert_eq!(
        CType::from_type(&Type::Var(osprey_types::VarId::from(0u32))),
        None
    );
}

#[test]
fn imports_use_c_bool_attributes_and_adapt_unit_returns() {
    let source = "extern fn host_flag(b: bool) -> bool\nextern fn host_log(b: bool) -> Unit\nfn flag(b) = host_flag(b)\nfn log(b) = host_log(b)\n";
    let (abi, ir) = abi_of(source);
    let out = with_host_abi(&ir, &abi).expect("adapted imports");
    assert!(
        out.contains("declare zeroext i1 @host_flag(i1 zeroext)"),
        "{out}"
    );
    assert!(
        out.contains("call zeroext i1 @host_flag(i1 zeroext %p0)"),
        "{out}"
    );
    assert!(out.contains("declare void @host_log(i1 zeroext)"), "{out}");
    assert!(
        out.contains("call void @host_log(i1 zeroext %p0)\n  ret i64 0"),
        "{out}"
    );
    assert!(!out.contains("call i64 @host_log("), "{out}");
}

#[test]
fn original_symbols_and_imports_cannot_shadow_the_boundary() {
    for (source, symbol) in [
        (
            "fn greet(n) = n + 1 ?: 0\nfn osprey_greet(n) = n + 2 ?: 0\n",
            "osprey_greet",
        ),
        ("fn osprey_main() = 4\n", "osprey_main"),
        ("extern fn osprey_main() -> int\n", "osprey_main"),
    ] {
        let (program, types, ir) = checked(source);
        let error = host_abi(&program, &types, &ir).expect_err("boundary name clash");
        assert!(error.contains(symbol), "{error}");
    }
}

#[test]
fn duplicate_namespace_export_names_are_rejected() {
    let name_a = osprey_ast::symbol::mangle(["app", "x_y"]);
    let name_b = osprey_ast::symbol::mangle(["app_x", "y"]);
    let source = format!("fn {name_a}() = 1\nfn {name_b}() = 2\n");
    let (program, types, ir) = checked(&source);
    let error = host_abi(&program, &types, &ir).expect_err("flattened names collide");
    assert!(error.contains("osprey_app_x_y"), "{error}");
}

#[test]
fn header_parameter_names_do_not_collide_with_c_keywords_or_macros() {
    let abi = HostAbi {
        exports: vec![export(
            "run",
            vec![("switch".into(), CType::Bool), ("class".into(), CType::Int)],
            CType::Unit,
        )],
        imports: vec![],
    };
    let h = header_for_target(&abi, "demo.osp", "ios");
    assert!(
        h.contains("void osprey_run(bool osprey_arg0, int64_t osprey_arg1);"),
        "{h}"
    );
}

#[test]
fn entry_replacement_rejects_multiple_or_commented_definitions() {
    for ir in [
        "; define i32 @main() {\n",
        "define i32 @main() {\n}\ndefine i32 @main() {\n}\n",
    ] {
        assert!(with_host_abi(ir, &HostAbi::default()).is_err(), "{ir}");
    }
}

#[test]
fn unsupported_extern_signatures_report_the_c_boundary_restriction() {
    for source in [
        "extern fn host_raw(handle: List<int>) -> int\n",
        "extern fn host_raw() -> List<int>\n",
        "extern fn host_raw(value: Unit) -> int\n",
        "extern fn host_raw(value: any) -> int\n",
    ] {
        let (program, types, ir) = checked(source);
        let error =
            host_abi(&program, &types, &ir).expect_err("unsupported extern must be rejected");
        assert!(
            error.contains("host_raw") && error.contains("C ABI") && error.contains("Unit"),
            "{error}"
        );
    }
}

#[test]
fn renaming_imports_preserves_strings_and_similarly_prefixed_symbols() {
    let (abi, ir) = abi_of("extern fn host_flag(b: bool) -> bool\nfn host_flag_more(b) = host_flag(b)\nfn text() = \"@host_flag( @main(\"\n");
    let out = with_host_abi(&ir, &abi).expect("adapted");
    assert!(out.contains("c\"@host_flag( @main(\\00\""), "{out}");
    assert!(out.contains("define i1 @host_flag_more("), "{out}");
}

#[test]
fn imported_or_defined_adapter_names_are_rejected() {
    for source in [
        "extern fn flag(b: bool) -> bool\nfn __osprey_host_flag() = 1\n",
        "extern fn flag(b: bool) -> bool\nextern fn __osprey_host_flag() -> int\n",
    ] {
        let (program, types, ir) = checked(source);
        let error = host_abi(&program, &types, &ir).expect_err("adapter name collision");
        assert!(error.contains("__osprey_host_flag"), "{error}");
    }
}

#[test]
fn initialization_symbols_and_c_header_names_are_reserved() {
    for symbol in [
        INIT_FUNCTION,
        INIT_STATE,
        INIT_STATUS,
        "INT64_MAX",
        "uint_fast64_t",
    ] {
        let source = format!("extern fn {symbol}() -> int\n");
        let (program, types, ir) = checked(&source);
        let error = host_abi(&program, &types, &ir).expect_err("reserved name");
        assert!(error.contains(symbol), "{error}");
    }
}

#[test]
fn exports_cannot_depend_on_a_handler_installed_only_by_main() {
    let source = "effect Alarm { ring: fn() -> int }\nfn ring() = perform Alarm.ring()\nfn relay() = ring()\nfn main() = handle Alarm\n ring => 7\nin relay()\n";
    let (program, types, ir) = checked(source);
    let error = host_abi(&program, &types, &ir).expect_err("export needs its own handler");
    assert!(
        error.contains("export") && error.contains("ring") && error.contains("Alarm.ring"),
        "{error}"
    );
}

#[test]
fn exports_with_their_own_handler_are_supported() {
    let (abi, _) = abi_of("effect Alarm { ring: fn() -> int }\nfn answer() = handle Alarm\n ring => 7\nin perform Alarm.ring()\n");
    assert_eq!(
        abi.exports
            .iter()
            .map(|e| e.c_name.as_str())
            .collect::<Vec<_>>(),
        ["osprey_answer"]
    );
}

#[test]
fn generated_header_and_ir_compile_with_clang() -> Result<(), Box<dyn std::error::Error>> {
    let source = "extern fn host_flag(b: bool) -> bool\nextern fn host_log(n: int) -> Unit\nfn flag(b) = host_flag(b)\nfn log(n) = host_log(n)\nfn score(n) = toFloat(n)\nfn text() = \"hello\"\n";
    let (abi, ir) = abi_of(source);
    for language in ["c-header", "c++-header"] {
        clang_accepts(
            &[
                "-ffreestanding",
                "-Werror",
                "-x",
                language,
                "-fsyntax-only",
                "-",
            ],
            &header_for_target(&abi, "demo.osp", "ios"),
        )?;
    }
    clang_accepts(
        &[
            "-target",
            "arm64-apple-ios17.0",
            "-Wno-override-module",
            "-x",
            "ir",
            "-S",
            "-emit-llvm",
            "-o",
            "-",
            "-",
        ],
        &with_host_abi(&ir, &abi)?,
    )
}

fn clang_accepts(args: &[&str], input: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(crate::c_compiler())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "clang rejected generated ABI:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn headers_accept_standard_c_macros_and_typedefs_as_parameters(
) -> Result<(), Box<dyn std::error::Error>> {
    let abi = HostAbi {
        exports: vec![export(
            "run",
            vec![
                ("INT64_MAX".into(), CType::Int),
                ("uint_fast64_t".into(), CType::Int),
                ("osprey_arg0".into(), CType::Int),
            ],
            CType::Unit,
        )],
        imports: vec![],
    };
    for language in ["c-header", "c++-header"] {
        clang_accepts(
            &[
                "-ffreestanding",
                "-Werror",
                "-x",
                language,
                "-fsyntax-only",
                "-",
            ],
            &header_for_target(&abi, "demo.osp", "ios"),
        )?;
    }
    Ok(())
}

#[test]
fn initialization_caches_status_and_rejects_reentrant_calls(
) -> Result<(), Box<dyn std::error::Error>> {
    use std::{fs, process::Command};
    let directory = std::env::temp_dir().join(format!("osprey-ios-init-{}", std::process::id()));
    fs::create_dir_all(&directory)?;
    let ir = "declare i32 @host_enter()\ndefine i32 @main() {\n %r = call i32 @host_enter()\n ret i32 %r\n}\n";
    fs::write(
        directory.join("entry.ll"),
        with_host_abi(ir, &HostAbi::default())?,
    )?;
    fs::write(directory.join("host.c"), "int osprey_main(void);\nstatic int attempts;\nint host_enter(void) { if (++attempts > 1) return 99; return osprey_main() == 1 ? 7 : 9; }\nint main(void) { int first = osprey_main(); int second = osprey_main(); return first == 7 && second == 7 && attempts == 1 ? 0 : 1; }\n")?;
    let executable = directory.join(format!("entry{}", std::env::consts::EXE_SUFFIX));
    let compiled = Command::new(crate::c_compiler())
        .arg(directory.join("entry.ll"))
        .arg(directory.join("host.c"))
        .arg("-o")
        .arg(&executable)
        .output()?;
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let status = Command::new(&executable).status()?;
    fs::remove_dir_all(&directory)?;
    assert!(
        status.success(),
        "initializer must execute once, cache status and reject recursion"
    );
    Ok(())
}
