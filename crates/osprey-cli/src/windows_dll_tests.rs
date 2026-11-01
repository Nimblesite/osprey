use super::super::tests::abi_of;
use super::*;

const SOURCE: &str = "extern fn host_slice(key: string) -> string\n\
    extern fn host_flag(b: bool) -> bool\n\
    fn fetch(key: string) -> string = host_slice(key)\n\
    fn flag(b: bool) -> bool = host_flag(b)\n\
    fn main() = print(fetch(\"x\"))\n";

fn adapted(source: &str) -> (String, Vec<Import>, HostAbi) {
    let (abi, ir) = abi_of(source);
    let library = super::super::with_host_abi(&ir, &abi).expect("host ABI");
    let (dll, slotted) = adapt(&library, &abi, true).expect("DLL boundary");
    (dll, slotted, abi)
}

/// [WINUI-HOST-IMPORTS] Each import becomes a slot that an exported binder fills, and the
/// declaration the host would have to define at link time is gone.
#[test]
fn imports_become_slots_the_host_binds() {
    let (dll, slotted, _) = adapted(SOURCE);
    let names: Vec<&str> = slotted.iter().map(|i| i.symbol.as_str()).collect();
    assert_eq!(names, ["host_slice", "host_flag"]);
    for wanted in [
        "@__osprey_slot_host_slice = internal global ptr null",
        "define internal i8* @host_slice(i8* %p0) {",
        "  %fn = load ptr, ptr @__osprey_slot_host_slice",
        "define dllexport void @osprey_import_host_slice(ptr %fn) {",
        "  store ptr %fn, ptr @__osprey_slot_host_slice",
    ] {
        assert!(dll.contains(wanted), "missing `{wanted}` in:\n{dll}");
    }
    assert!(
        !dll.lines()
            .any(|line| line.starts_with("declare ") && line.contains("@host_slice(")),
        "the import is still a link-time declaration:\n{dll}"
    );
}

/// [WINUI-HOST-IMPORTS] An import the DLL's own runtime archive defines, such as the TLS session
/// or the file server [FFI-TLS-CLIENT] [FFI-FILE-SERVER], stays a link-time declaration: the
/// archive is linked into the DLL, and no host answers it.
#[test]
fn runtime_imports_link_from_the_archive_not_the_host() {
    let source = "extern fn osprey_tls_close(handle: int) -> int
        extern fn osprey_media_render(audio: string) -> string
        extern fn host_slice(key: string) -> string
        fn main() = print(toString(osprey_tls_close(1)) + osprey_media_render(\"a\") + host_slice(\"x\"))
";
    let (dll, slotted, _) = adapted(source);
    let names: Vec<&str> = slotted.iter().map(|i| i.symbol.as_str()).collect();
    assert_eq!(names, ["host_slice"]);
    for runtime in ["osprey_tls_close", "osprey_media_render"] {
        assert!(
            dll.lines()
                .any(|line| line.starts_with("declare ") && line.contains(&format!("@{runtime}("))),
            "`{runtime}` should stay a declaration the archive defines:
{dll}"
        );
        assert!(
            !dll.contains(&format!("@osprey_import_{runtime}(")),
            "{dll}"
        );
    }
}

/// A slot keeps the C bool convention the import adapter calls it with.
#[test]
fn bool_slots_keep_the_c_bool_convention() {
    let (dll, _, _) = adapted(SOURCE);
    assert!(
        dll.contains("define internal zeroext i1 @host_flag(i1 zeroext %p0) {"),
        "{dll}"
    );
    assert!(dll.contains("call zeroext i1 %fn(i1 zeroext %p0)"), "{dll}");
}

/// [WINUI-HOST-DLL] The entry refuses to run while a slot is empty, then runs the program once.
#[test]
fn the_entry_checks_every_slot_before_running_the_program() {
    let (dll, _, _) = adapted(SOURCE);
    let entry = dll
        .split("define dllexport i32 @osprey_main() {")
        .nth(1)
        .expect("exported entry");
    for wanted in [
        "%slot0 = load ptr, ptr @__osprey_slot_host_slice",
        "%slot1 = load ptr, ptr @__osprey_slot_host_flag",
        "%status = call i32 @__osprey_dll_entry()",
        &format!("ret i32 {UNBOUND}"),
    ] {
        assert!(entry.contains(wanted), "missing `{wanted}` in:\n{entry}");
    }
    assert!(dll.contains("define i32 @__osprey_dll_entry() {"), "{dll}");
}

/// Only the entry, the C exports and the binders are exported from the DLL.
#[test]
fn only_the_boundary_is_dllexported() {
    let (dll, slotted, abi) = adapted(SOURCE);
    let exported: Vec<&str> = dll.lines().filter(|l| l.contains("dllexport")).collect();
    assert_eq!(
        exported.len(),
        1 + abi.exports.len() + slotted.len(),
        "{exported:#?}"
    );
    for export in &abi.exports {
        let needle = format!(" @{}(", export.c_name);
        assert!(
            exported.iter().any(|line| line.contains(&needle)),
            "{needle}"
        );
    }
}

/// A function that already holds a binder's name is refused rather than shadowed.
#[test]
fn a_binder_name_already_defined_is_refused() {
    let (abi, ir) = abi_of(SOURCE);
    let library = super::super::with_host_abi(&ir, &abi).expect("host ABI");
    let taken =
        format!("{library}\ndefine i64 @osprey_import_host_flag(i64 %x) {{\n  ret i64 %x\n}}\n");
    let refused = adapt(&taken, &abi, true).expect_err("clash");
    assert!(
        refused.contains("`osprey_import_host_flag` is already defined"),
        "{refused}"
    );
}

/// The header documents a binder per slot in place of functions the host must define.
#[test]
fn the_header_declares_a_binder_per_slot() {
    let (_, slotted, abi) = adapted(SOURCE);
    let header = header(&abi.exports, &slotted, "app.osp", "windows-x64");
    for wanted in [
        "int32_t osprey_main(void);",
        "void osprey_import_host_slice(const char * (*fn)(const char *));",
        "void osprey_import_host_flag(bool (*fn)(bool));",
        &format!("returns {UNBOUND} while any slot is still empty"),
    ] {
        assert!(header.contains(wanted), "missing `{wanted}` in:\n{header}");
    }
    assert!(!header.contains("must define with C linkage"), "{header}");
    assert!(header.ends_with(HEADER_CLOSE), "{header}");
}

/// The internal Unit adapter calls the C-void import slot, preserving both signatures.
#[test]
fn unit_slots_preserve_the_internal_adapter_and_c_void_boundary() {
    let source = "extern fn host_notice(value: int)\n\
        fn publish(value) = host_notice(value)\n\
        fn main() = publish(1)\n";
    let (dll, slotted, _) = adapted(source);
    assert_eq!(slotted.len(), 1);
    assert_eq!(slotted.first().map(|import| import.ret), Some(CType::Unit));
    for wanted in [
        "define internal void @host_notice(i64 %p0)",
        "call void %fn(i64 %p0)",
        "define internal i64 @__osprey_host_host_notice(i64 %p0)",
        "call void @host_notice(i64 %p0)",
        "ret i64 0",
    ] {
        assert!(dll.contains(wanted), "missing `{wanted}` in:\n{dll}");
    }
}
