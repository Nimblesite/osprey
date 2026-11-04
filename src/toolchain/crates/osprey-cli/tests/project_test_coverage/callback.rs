//! Inlined record callbacks count invocations, including across files (#296).
use super::{covered_run, file_lines, project, write, MANIFEST};
use std::path::PathBuf;

const STEPS: &str = r"// Implements [TESTING-COVERAGE-CODEGEN].
namespace proof

type Item =
    value : string

type Section =
    records : List<Item>

module Steps
    entries section = mapList (section.records, \item => Item(value = item.value))
    export gather sections = mapList (sections, entries)
    unusedMapper section = mapList (section.records, \item => Item(value = item.value))
";

const MAIN: &str = r#"namespace proof

test ("one section transforms both records", \() =>
    rows = Steps::gather [Section(records = [Item(value = "first"), Item(value = "second")])]
    expect ((listGet (listGet rows 0 ?: []) 0 ?: Item(value = "missing")).value, "first")
    expect ((listGet (listGet rows 0 ?: []) 1 ?: Item(value = "missing")).value, "second"))

test ("three sections invoke three transforms", \() =>
    rows = Steps::gather [Section(records = [Item(value = "first")]), Section(records = [Item(value = "second")]), Section(records = [Item(value = "third")])]
    expect (length rows, 3)
    expect ((listGet (listGet rows 0 ?: []) 0 ?: Item(value = "missing")).value, "first")
    expect ((listGet (listGet rows 1 ?: []) 0 ?: Item(value = "missing")).value, "second")
    expect ((listGet (listGet rows 2 ?: []) 0 ?: Item(value = "missing")).value, "third"))

test ("empty input never invokes the mapper", \() =>
    rows = Steps::gather (filterList ([], \section => true))
    expect (length rows, 0))
"#;

fn callback_project(memory: &str) -> PathBuf {
    let root = project(&format!("record_callbacks_{memory}"));
    write(
        &root.join("osprey.toml"),
        &MANIFEST.replace("\"app\"", "\"proof\""),
    );
    write(&root.join("framework/src/mapper.ospml"), STEPS);
    write(&root.join("tests/main.ospml"), MAIN);
    root
}

fn check_callbacks(memory: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = callback_project(memory);
    let (output, report) = covered_run(&root, memory)?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("# tests=3 passed=3 failed=0 skipped=0"),
        "{stdout}"
    );
    assert_eq!(
        file_lines(&report, "mapper.ospml"),
        vec![(11, 4), (12, 3), (13, 0)]
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn inlined_record_callbacks_count_each_invocation_across_files(
) -> Result<(), Box<dyn std::error::Error>> {
    for memory in ["gc", "arc"] {
        check_callbacks(memory)?;
    }
    Ok(())
}
