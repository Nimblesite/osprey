//! The CLI defaults to managed memory and rejects retired backend spellings.
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
fn retired_memory_backends_are_rejected_on_native_and_wasm() {
    let source = temp_osp("retired_memory", "print(\"managed memory\")\n");
    for target in ["native", "wasm32"] {
        for backend in ["none", "default"] {
            let output = osprey()
                .arg(&source)
                .args([
                    "--check",
                    &format!("--target={target}"),
                    &format!("--memory={backend}"),
                ])
                .output()
                .expect("run compiler");
            assert!(!output.status.success(), "{target} accepted {backend}");
            assert!(String::from_utf8_lossy(&output.stderr)
                .contains(&format!("unknown memory backend '{backend}'")));
        }
    }
}
