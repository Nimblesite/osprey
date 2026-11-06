use super::{osprey, temp_dir};

#[test]
fn standalone_suites_keep_writes_out_of_the_callers_directory() {
    for memory in ["gc", "arc"] {
        let caller = temp_dir(&format!("suite_scratch_{memory}"));
        let source = caller.join("writer.test.osp");
        for expected in [7, 8] {
            std::fs::write(&source, format!(
                "test(\"writes\", || => expect(writeFile(\"artifact.txt\", \"scratch\") ?: -1, {expected}))\n"
            )).expect("write suite");
            let output = osprey()
                .current_dir(&caller)
                .args(["test", "writer.test.osp", &format!("--memory={memory}")])
                .output()
                .expect("run suite");
            assert_eq!(
                output.status.success(),
                expected == 7,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !caller.join("artifact.txt").exists(),
                "standalone suite wrote into caller directory ({memory}, expected={expected})"
            );
        }
        std::fs::remove_dir_all(caller).expect("remove fixture");
    }
}
