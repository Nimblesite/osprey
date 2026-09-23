//! Native regression for #184: full and partial curried calls retain effects.

use std::path::Path;
use std::process::Command;

#[test]
fn ml_curried_effects_reach_value_and_resuming_handlers_in_every_memory_mode() {
    let directory = std::env::temp_dir().join(format!("osprey_curried_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create test directory");
    let path = directory.join("curried_effects.ospml");
    std::fs::write(&path, include_str!("fixtures/curried_effects.ospml"))
        .expect("write source fixture");

    for memory in ["default", "gc", "arc"] {
        let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .arg(&path)
            .args(["--run", &format!("--memory={memory}")])
            .output()
            .expect("run Osprey compiler");
        assert!(
            output.status.success(),
            "{memory}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "direct=10,control=10,audit=4\n",
            "{memory}"
        );
    }
}
