//! Memory management is automatic; opting out requires an explicit CLI flag.
use super::{osprey, temp_osp};

#[test]
fn an_omitted_memory_flag_links_the_gc_runtime() {
    let source = temp_osp(
        "managed_default",
        "extern fn osp_gc_collections() -> int\nprint(osp_gc_collections())\n",
    );
    let output = osprey()
        .arg(source)
        .args(["--run", "--quiet"])
        .output()
        .expect("run compiler");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
}

#[test]
fn unmanaged_execution_requires_the_explicit_none_option() {
    let source = temp_osp("unmanaged_opt_in", "print(\"explicit opt-out\")\n");
    let output = osprey()
        .arg(&source)
        .args(["--run", "--quiet", "--memory=none"])
        .output()
        .expect("run compiler");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"explicit opt-out\n");
    let ambiguous = osprey()
        .arg(source)
        .args(["--check", "--memory=default"])
        .output()
        .expect("run compiler");
    assert!(
        !ambiguous.status.success(),
        "ambiguous allocator option accepted"
    );
}
