//! Every scalar-signature function reaches the library boundary, or the
//! build fails and names it (#281).
use super::selection::{host_abi, host_abi_scoped};
use super::tests::{abi_of, checked};
use super::ExportScope;
use std::fmt::Write as _;

#[test]
fn numeric_exports_and_imports_execute_against_real_c() -> Result<(), Box<dyn std::error::Error>> {
    // [FFI-NUMERIC-WIDTH] Real C calls both directions, including high bits and signs.
    let mut source = String::new();
    let mut host = String::from("#include <stdint.h>\n#include <assert.h>\nvoid osp_mem_boot(void){}\nvoid osp_prof_boot(void){}\nvoid osp_release(void *p){(void)p;}\n");
    let mut assertions = String::from("int main(void){\n");
    for (i, (ty, c, values)) in [
        ("int8", "int8_t", "-128,127"),
        ("int16", "int16_t", "-32768,32767"),
        ("int32", "int32_t", "INT32_MIN,INT32_MAX"),
        ("int", "int64_t", "INT64_MIN,INT64_MAX"),
        ("uint8", "uint8_t", "0,255"),
        ("uint16", "uint16_t", "0,65535"),
        ("uint32", "uint32_t", "0,UINT32_MAX"),
        ("float16", "uint16_t", "0x8000,0x7bff"),
        ("float32", "float", "-123.5f,0x1.fffffep127f"),
        ("float", "double", "-123.5,0x1.fffffffffffffp1023"),
    ]
    .into_iter()
    .enumerate()
    {
        writeln!(
            source,
            "extern fn host{i}(value: {ty}) -> {ty}\nfn relay{i}(value) = host{i}(value)"
        )?;
        writeln!(
            host,
            "{c} osprey_relay{i}({c});\n{c} host{i}({c} value){{return value;}}"
        )?;
        for value in values.split(',') {
            writeln!(
                assertions,
                "assert(osprey_relay{i}(({c})({value})) == ({c})({value}));"
            )?;
        }
    }
    writeln!(host, "{assertions}return 0;}}")?;
    let (abi, ir) = abi_of(&source);
    let extended = !cfg!(any(
        target_os = "windows",
        all(target_os = "linux", target_arch = "aarch64")
    ));
    let adapted = super::with_host_abi_for_target(&ir, &abi, extended)?;
    let root = std::env::temp_dir().join(format!("osprey-numeric-c-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("numeric.ll"), adapted)?;
    std::fs::write(root.join("host.c"), host)?;
    let build = std::process::Command::new("clang")
        .current_dir(&root)
        .args([
            "-O2",
            "-Wno-override-module",
            "numeric.ll",
            "host.c",
            "-o",
            "numeric",
        ])
        .output()?;
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(std::process::Command::new(root.join("numeric"))
        .status()?
        .success());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn every_numeric_width_crosses_the_host_boundary_without_widening() -> Result<(), String> {
    // [FFI-NUMERIC-WIDTH] Exercise both imports and exports of every numeric row.
    for (ty, c, llvm, extension) in [
        ("int8", "int8_t", "i8", "signext "),
        ("int16", "int16_t", "i16", "signext "),
        ("int32", "int32_t", "i32", ""),
        ("int", "int64_t", "i64", ""),
        ("uint8", "uint8_t", "i8", "zeroext "),
        ("uint16", "uint16_t", "i16", "zeroext "),
        ("uint32", "uint32_t", "i32", ""),
        ("float16", "uint16_t", "i16", "zeroext "),
        ("float32", "float", "float", ""),
        ("float", "double", "double", ""),
    ] {
        let python = osprey_syntax::parse_program_with_flavor(
            &format!("def identity(value: {ty}) -> {ty}:\n    return value\n"),
            osprey_syntax::Flavor::Python,
        );
        assert!(python.errors.is_empty(), "{ty}: {:?}", python.errors);
        assert!(
            osprey_types::check_program(&python.program).is_empty(),
            "{ty}"
        );
        let (abi, ir) = abi_of(&format!(
            "extern fn host(value: {ty}) -> {ty}\nfn relay(value) = host(value)\n"
        ));
        assert_eq!(abi.exports.len(), 1, "missing {ty} export");
        assert_eq!(abi.exports.first().map(|export| export.ret.c()), Some(c));
        for extended in [true, false] {
            let ext = if extended { extension } else { "" };
            let adapted = super::with_host_abi_for_target(&ir, &abi, extended)?;
            for expected in [
                format!(
                    "declare {ext}{llvm} @host({})",
                    format!("{llvm} {ext}").trim_end()
                ),
                format!("define {ext}{llvm} @osprey_relay({llvm} {ext}%p0)"),
                format!("call {ext}{llvm} @host({llvm} {ext}%p0)"),
            ] {
                assert!(
                    adapted.contains(&expected),
                    "{ty}: missing {expected}\n{adapted}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn borrowed_string_import_references_use_the_owned_adapter() -> Result<(), String> {
    let (abi, ir) = abi_of("extern fn hostText(index:int)->string\nfn relay(reader:(int)->string,index:int)=reader(index)\nlet result=relay(hostText,1)\nfn read(index:int)=relay(hostText,index)\n");
    let adapted = super::with_host_abi(&ir, &abi)?;
    assert_owned_adapter(&adapted);
    assert_eq!(
        adapted.matches("@hostText").count(),
        2,
        "only the raw declaration and adapter call may name the borrowed import"
    );
    Ok(())
}

fn assert_owned_adapter(adapted: &str) {
    crate::testkit::contents(
        adapted,
        &[
            "@__osprey_host_hostText",
            "%owned = call i8* @osp_host_string_copy(i8* %r)",
            "define i32 @osprey_host_abi_version()",
            "define void @osprey_release_string(i8* %value)",
            "call void @osp_release(i8* %value)",
        ],
        &[],
    );
}

#[test]
fn entry_only_unit_imports_keep_the_host_adapter() -> Result<(), String> {
    let parsed =
        osprey_syntax::parse_program("extern fn host_log(b: bool) -> Unit\nhost_log(true)\n");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let (ir, _) = super::source(
        &parsed.program,
        "entry.osp",
        "macos",
        true,
        &ExportScope::EntryOnly,
    )?;
    assert_entry_unit_adapter(&ir);
    Ok(())
}

fn assert_entry_unit_adapter(ir: &str) {
    crate::testkit::contents(
        ir,
        &[
            "declare void @host_log(i1 zeroext)",
            "call void @host_log(i1 zeroext %p0)\n  ret i64 0",
        ],
        &["call i64 @host_log("],
    );
}

#[test]
fn a_scalar_function_codegen_did_not_define_is_refused_not_dropped() {
    // [IOS-HOST-ABI] The archive and header would silently lack `greet`.
    let (program, types, ir) =
        checked("fn greet(name: string) -> string = name\nfn plain(n: int) -> int = n\n");
    let inlined = ir.replace(" @greet(", " @greet_inlined(");
    let error = host_abi(&program, &types, &inlined).expect_err("undefined export");
    assert!(
        error.contains("library export `greet`") && !error.contains("`plain`"),
        "{error}"
    );
}

#[test]
fn signal_queries_in_the_call_graph_keep_the_export() {
    // [IOS-HOST-ABI] [STAGE-SIGNALS-CACHE]
    let (abi, _) = abi_of(
        "static effect Signal<T> { read: fn() -> T }\n\
         type Tally = { tally: int }\n\
         fn tallied() -> Tally = perform Signal<Tally>.read()\n\
         fn ids() = signalDependencies(tallied)\n\
         fn go(saved: string) -> string = \"${join(ids(), \",\")}${saved}\"\n\
         fn snap(saved: string) -> string = {\n\
             handle static Signal<Tally> { read => Tally { tally: 3 } }\n\
             \"${signalSnapshot(tallied).value.tally}${saved}\"\n\
         }\n\
         fn plain(saved: string) -> string = \"p${saved}\"\n",
    );
    let names: Vec<&str> = abi.exports.iter().map(|e| e.c_name.as_str()).collect();
    assert_eq!(names, ["osprey_go", "osprey_snap", "osprey_plain"]);
}

#[test]
fn only_the_entry_sources_functions_cross_the_boundary() {
    // [IOS-HOST-ABI] Lines 1-2 stand for the entry source. `helper` belongs to
    // another file: it is neither exported nor required to have a symbol.
    let (program, types, ir) = checked(
        "fn start(n: int) -> int = n\nfn dispatch(s: string) -> string = s\nfn helper(n: int) -> int = n\n",
    );
    let inlined = ir.replace(" @helper(", " @helper_inlined(");
    let entry = ExportScope::EntrySource(Some(1..=2));
    let abi = host_abi_scoped(&program, &types, &inlined, &entry).expect("entry exports");
    let names: Vec<&str> = abi.exports.iter().map(|e| e.c_name.as_str()).collect();
    assert_eq!(names, ["osprey_start", "osprey_dispatch"]);
}

#[test]
fn borrowed_export_strings_are_copied_and_released_around_the_call() -> Result<(), String> {
    let (abi, ir) = abi_of(
        "fn echo(text: string) = text\nfn combineHostInputs(left: string, right) = left + right\n",
    );
    let adapted = super::with_host_abi(&ir, &abi)?;
    crate::testkit::contents(&adapted, &[
        "declare i8* @osp_host_string_copy(i8*)",
        "%owned_p0 = call i8* @osp_host_string_copy(i8* %p0)",
        "%owned_p1 = call i8* @osp_host_string_copy(i8* %p1)",
        "%r = call i8* @echo(i8* %owned_p0)\n  call void @osp_release(i8* %owned_p0)\n  ret i8* %r",
        "%r = call i8* @combineHostInputs(i8* %owned_p0, i8* %owned_p1)\n  call void @osp_release(i8* %owned_p0)\n  call void @osp_release(i8* %owned_p1)\n  ret i8* %r",
    ], &[]);
    Ok(())
}
