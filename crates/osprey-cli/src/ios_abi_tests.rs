use super::*;
use std::path::{Path, PathBuf};

const SOURCE: &str = "extern fn host_log(message: string) -> int\n\
    extern fn host_raw(handle: List<int>) -> int\n\
    fn greet(name: string) -> string = \"hello ${name}\"\n\
    fn flag(b: bool) -> bool = !b\n\
    fn shout(s: string) = print(s)\n\
    fn scaled(n: int) -> float = toFloat(n) * 1.5\n\
    fn twice(x) = x\n\
    fn total(a: int, b: int) -> int = a + b ?: 0\n\
    fn count(xs: List<int>) -> int = listLength(xs)\n\
    fn main() = print(\"${host_log(greet(\\\"x\\\"))}\")\n";

fn checked(source: &str) -> (Program, ProgramTypes, String) {
    let program = osprey_syntax::parse_program(source).program;
    assert!(osprey_types::check_program(&program).is_empty(), "fixture types");
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
        ["osprey_greet", "osprey_flag", "osprey_shout", "osprey_scaled", "osprey_total"]
    );
    // `twice` is generic (inlined, no symbol), `count` takes a List, and
    // `main` is the entry: none of them is an export.
    assert!(abi.exports.iter().all(|e| e.symbol == e.source_name));
    let shout = abi.exports.iter().find(|e| e.symbol == "shout").expect("shout");
    assert_eq!(shout.ret, CType::Unit);
    assert_eq!(shout.params, vec![("s".to_string(), CType::Str)]);
    // Only the scalar extern is an import; the List one is not C-shaped.
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
    assert!(out.contains("define void @osprey_shout(i8* %p0) {\n  call i64 @shout(i8* %p0)\n  ret void"));
    assert!(out.contains("define i8* @osprey_greet(i8* %p0) {"));
    assert!(out.contains("define double @osprey_scaled(i64 %p0) {"));
    assert!(out.contains("define i64 @osprey_total(i64 %p0, i64 %p1) {"));
    assert!(with_host_abi("; no entry here", &abi).is_err());
}

#[test]
fn header_declares_entry_exports_and_imports_in_c() {
    // [IOS-HOST-ABI]
    let (abi, _) = abi_of(SOURCE);
    let h = header(&abi, "demo.osp");
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
    assert!(!h.contains("host_raw"), "List-typed extern is not C-shaped");
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
    let greet = abi.exports.iter().find(|e| e.source_name == "app::greet").expect("greet");
    assert_eq!(greet.c_name, "osprey_app_greet");
    assert!(greet.symbol.starts_with("__osp_"), "forwards to the mangled symbol");
    assert!(with_host_abi(&ir, &abi).expect("thunked").contains(&format!("call i8* @{}(i8* %p0)", greet.symbol)));
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
fn c_types_map_only_scalars() {
    let con = |name: &str| Type::Con { name: name.to_string(), args: vec![] };
    assert_eq!(CType::from_type(&con("int")), Some(CType::Int));
    assert_eq!(CType::from_type(&con("Unit")), Some(CType::Unit));
    assert_eq!(CType::from_type(&con("Point")), None);
    assert_eq!(CType::from_type(&Type::Con { name: "List".to_string(), args: vec![con("int")] }), None);
    assert_eq!(CType::from_type(&Type::Var(osprey_types::VarId::from(0u32))), None);
}

#[test]
fn imports_use_c_bool_attributes_and_adapt_unit_returns() {
    let source = "extern fn host_flag(b: bool) -> bool\nextern fn host_log(b: bool) -> Unit\nfn flag(b) = host_flag(b)\nfn log(b) = host_log(b)\n";
    let (abi, ir) = abi_of(source);
    let out = with_host_abi(&ir, &abi).expect("adapted imports");
    assert!(out.contains("declare zeroext i1 @host_flag(i1 zeroext)"), "{out}");
    assert!(out.contains("call zeroext i1 @host_flag(i1 zeroext %p0)"), "{out}");
    assert!(out.contains("declare void @host_log(i1 zeroext)"), "{out}");
    assert!(out.contains("call void @host_log(i1 zeroext %p0)\n  ret i64 0"), "{out}");
    assert!(!out.contains("call i64 @host_log("), "{out}");
}

#[test]
fn original_symbols_and_imports_cannot_shadow_the_boundary() {
    for (source, symbol) in [
        ("fn greet(n) = n + 1 ?: 0\nfn osprey_greet(n) = n + 2 ?: 0\n", "osprey_greet"),
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
        exports: vec![export("run", vec![("switch".into(), CType::Bool), ("class".into(), CType::Int)], CType::Unit)],
        imports: vec![],
    };
    let h = header(&abi, "demo.osp");
    assert!(h.contains("void osprey_run(bool osprey_arg0, int64_t osprey_arg1);"), "{h}");
}

#[test]
fn entry_replacement_rejects_multiple_or_commented_definitions() {
    for ir in ["; define i32 @main() {\n", "define i32 @main() {\n}\ndefine i32 @main() {\n}\n"] {
        assert!(with_host_abi(ir, &HostAbi::default()).is_err(), "{ir}");
    }
}
