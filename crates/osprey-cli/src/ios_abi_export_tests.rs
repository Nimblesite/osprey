//! Every scalar-signature function reaches the library boundary, or the
//! build fails and names it (#281).
use super::selection::{host_abi, host_abi_selected};
use super::tests::{abi_of, checked};
use super::ExportScope;

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
    let abi = host_abi_selected(&program, &types, &inlined, &entry).expect("entry exports");
    let names: Vec<&str> = abi.exports.iter().map(|e| e.c_name.as_str()).collect();
    assert_eq!(names, ["osprey_start", "osprey_dispatch"]);
}
