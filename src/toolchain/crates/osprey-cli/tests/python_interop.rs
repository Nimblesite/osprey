//! Execute the Python package against the compiler built by Cargo.

#[path = "common/effect_execution.rs"]
mod effect_execution;

const COMPREHENSION_SCOPE: &str = r#"# osprey: flavor=python
def main():
    callbacks = map(range(1), lambda _: lambda xs: len(xs) + 5)
    a = [len([1, 2]) for len in callbacks if len([1]) > 0]
    b = [range(7) for range in map(range(1), lambda _: lambda x: x + 3)]
    c = [value for len in map(range(1), lambda _: lambda xs: range(4, 6)) for value in len([4, 5])]
    d = [x for x in map(range(1), lambda _: range(6, 8)) for x in x]
    return print(f"{unwrap_or(listGet(a, 0), -1)}|{unwrap_or(listGet(b, 0), -1)}|{len(toList(range(3)))}|{unwrap_or(listGet(c, 0), -1)}|{unwrap_or(listGet(d, 0), -1)}")
"#;

#[test]
fn comprehension_aliases_execute_in_their_lexical_scope() {
    effect_execution::assert_flavored_output(
        "python_comprehension_scope",
        "py",
        COMPREHENSION_SCOPE,
        "7|10|3|4|6\n",
    );
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn python_interoperability() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let output = std::process::Command::new("python3")
        .args([
            "-m",
            "unittest",
            "discover",
            "-s",
            "src/toolchain/python/tests",
            "-v",
        ])
        .current_dir(&root)
        .env("PYTHONPATH", root.join("src/toolchain/python"))
        .env("OSPREY", env!("CARGO_BIN_EXE_osprey"))
        .output()
        .expect("run Python interop tests");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn python_target_rejects_unsupported_host() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(root.join("src/osprey/examples/python/interop.osp"))
        .args(["--check", "--target=python"])
        .output()
        .expect("check Python target");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Linux or macOS"));
}

/// [FLAVOR-PYTHON-TYPING], [FLAVOR-PYTHON-EXPRESSIONS]: inferred polymorphic
/// bindings, higher-order closures, `Callable` shapes, a PEP 695 alias, list and
/// map lookups that stay `Result`s, and lexical shadowing — one program, every
/// value printed exactly.
const CLOSURES_ALIASES_SHADOWING: &str = r#"from testing import Check
from typing import Callable

type Label = str


def identity(x):
    return x


def constly(value):
    return lambda ignored: value


def apply(callback: Callable[[str], int]):
    return callback("nested")


def twice(step):
    return lambda x: step(step(x))


def total(items):
    return foldList(items, 0, lambda acc, item: acc + item)


def label(value: Label) -> Label:
    return f"[{value}]"


def status(text):
    match parseInt(text):
        case Success(value=value):
            return f"ok:{value}"
        case Error(message=message):
            return "bad"


def carried(text, separator):
    match split(text, separator):
        case Success(value=parts):
            return Success(value=join(filterList(parts, lambda part: part != ""), ":"))
        case Error(message=message):
            return Error(message=message)



def work():
    alias = identity
    held: Callable[[str], int] = constly(9)
    nested = apply
    quad = twice(twice(lambda n: n + 1))
    outer = 5
    bump = lambda outer: outer + 1
    level = lambda x: lambda x: x + 1
    inner = level(100)
    numbers = [alias(7), held("ignored")]
    words = {"answer": alias("seven"), "nested": identity("x")}
    grid = [[1, 2], [3, 4]]
    flat = foldList(grid, [], lambda acc, row: listConcat(acc, row))
    evens = filterList(flat, lambda value: value % 2 == 0)
    shadowed = mapList(filterList(evens, lambda value: value >= 2), lambda value: value * 10)
    columns = [index for index in range(len(flat)) if index > 0]
    totals = {"flat": total(flat), "evens": total(evens)}
    print(f"{numbers[0]}:{numbers[1]}:{words["answer"]}:{words["nested"]}:{identity(True)}:{alias(alias(5))}:{label(alias("tag"))}\n{nested(held)}:{quad(0)}:{bump(41)}:{outer}:{inner(1)}:{total(flat)}:{len(evens)}:{len(columns)}\n{status("42")}:{status("bad")}:{carried("a,,b", ",")}:{carried("x", "")}\n{shadowed[0]}:{unwrap_or(mapGet(totals, "flat"), -1)}:{unwrap_or(mapGet(totals, "absent"), -1)}:{mapGet(totals, "absent")}")


def main():
    match Check.strictly(work):
        case Success():
            return None
        case Error(message=message):
            return print(f"unexpected arithmetic failure: {message}")
"#;

#[test]
fn polymorphic_closures_aliases_and_shadowing_keep_every_value() {
    assert_project_output(
        "python_closures",
        &[("main.py", CLOSURES_ALIASES_SHADOWING)],
        "Success(7):Success(9):Success(seven):Success(x):true:5:[tag]\n\
         9:4:42:5:2:10:2:3\n\
         ok:42:bad:Success(a:b):Error(split: separator must not be empty)\n\
         Success(20):10:-1:Error(mapGet: key not found)\n",
    )
    .expect("loaded Python closures run under GC and ARC");
}

/// [FLAVOR-PYTHON-EXPRESSIONS], [ERR-PAYLOAD]: both `Result` arms survive
/// aliasing, doubling, nested list/map lookup and a fold; a `Success` payload is
/// the bare value, an `Error` keeps its exact message, and `unwrap_or` is the
/// only unbounded fallback.
const RESULT_PIPELINE: &str = r#"from testing import Check
def read_int(text):
    return parseInt(text)


def doubled(result):
    match result:
        case Success(value=value):
            return Success(value=value * 2)
        case Error(message=message):
            return Error(message=message)


def status(result):
    match result:
        case Success(value=value):
            return f"ok:{value}"
        case Error(message=message):
            return "error"


def parse_pair(text, separator):
    match split(text, separator):
        case Success(value=parts):
            return Success(value=join(parts, "-"))
        case Error(message=message):
            return Error(message=message)


def head_of(mapping, key):
    match mapGet(mapping, key):
        case Success(value=items):
            return listGet(items, 0)
        case Error(message=message):
            return Error(message=message)


def summarize(texts):
    return foldList(texts, "", lambda acc, text: f"{acc}{status(doubled(read_int(text)))}|")



def work():
    read = read_int
    lookup = head_of
    entries = {"a": [1, 2], "b": [3]}
    week = ["1", "4", "x", "7"]
    empty: list[int] = []
    print(f"{status(read("4"))}:{status(read("no"))}:{status(doubled(read("4")))}:{status(doubled(read("no")))}\n{unwrap_or(doubled(read("no")), -1)}:{unwrap_or(doubled(read("4")), -1)}:{lookup(entries, "a")}:{lookup(entries, "b")}\n{lookup(entries, "missing")}:{parse_pair("a,b", ",")}:{parse_pair("a,b", "")}\n{summarize(week)}:{read("4")}:{status(listGet(empty, 0))}")


def main():
    match Check.strictly(work):
        case Success():
            return None
        case Error(message=message):
            return print(f"unexpected arithmetic failure: {message}")
"#;

#[test]
fn result_success_and_error_payloads_survive_the_whole_pipeline() {
    assert_project_output(
        "python_result_pipeline",
        &[("main.py", RESULT_PIPELINE)],
        "ok:4:error:ok:8:error\n\
         -1:8:Success(1):Success(3)\n\
         Error(mapGet: key not found):Success(a-b):Error(split: separator must not be empty)\n\
         ok:2|ok:8|error|ok:14|:Success(4):error\n",
    )
    .expect("loaded Python Results run under GC and ARC");
}

/// [FLAVOR-PYTHON-INTEROP]: three Python-flavor files in one project, each its
/// own namespace/module contribution, imported both as modules and by a direct
/// member name. Exported closures, folds, `Result`s and map lookups cross the
/// module boundary and still print their exact host values.
const PROJECT_CALC: &str = r#"# osprey: flavor=python
# osprey: namespace=fixture
# osprey: module=Calc
__all__ = ["identity", "twice", "total", "carried", "status"]


def identity(x):
    return x


def twice(step):
    return lambda x: step(step(x))


def total(items):
    return foldList(items, 0, lambda acc, item: acc + item)


def carried(text, separator):
    match split(text, separator):
        case Success(value=parts):
            return Success(value=join(filterList(parts, lambda part: part != ""), ":"))
        case Error(message=message):
            return Error(message=message)


def status(text):
    match parseInt(text):
        case Success(value=value):
            return f"ok:{value}"
        case Error(message=message):
            return "error"
"#;

const PROJECT_WORDS: &str = r#"# osprey: flavor=python
# osprey: namespace=fixture
# osprey: module=Words
__all__ = ["caption", "grid_total", "pick"]


def caption(value, suffix):
    return f"[{value}{suffix}]"


def grid_total(rows):
    return foldList(rows, 0, lambda acc, row: acc + foldList(row, 0, lambda running, item: running + item))


def pick(mapping, key):
    return mapGet(mapping, key)
"#;

const PROJECT_MAIN: &str = r#"# osprey: flavor=python
from testing import Check
from fixture import Calc, Words
from fixture.Words import caption


def work():
    step = Calc.twice(lambda n: n + 4)
    rows = [[1, 2], [3, 4]]
    totals = {"rows": Words.grid_total(rows), "step": step(1)}
    print(f"{Calc.identity(7)}:{Calc.status("42")}:{Calc.status("x")}:{step(1)}\n{caption("rows", "=")}:{Words.grid_total(rows)}:{Calc.total([1, 2, 3, 4])}\n{Calc.carried("a,,b", ",")}:{Calc.carried("x", "")}:{Words.pick(totals, "rows")}:{Words.pick(totals, "missing")}")


def main():
    match Check.strictly(work):
        case Success():
            return None
        case Error(message=message):
            return print(f"unexpected arithmetic failure: {message}")
"#;

/// Write the manifest and `files` into a temp project, run it under GC and ARC,
/// assert `expected` byte for byte, and require a clean ARC exit on ARC.
fn assert_project_output(
    name: &str,
    files: &[(&str, &str)],
    expected: &str,
) -> std::io::Result<()> {
    let directory = std::env::temp_dir().join(format!(
        "osprey_python_project_{name}_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(directory.join("framework"))?;
    let check = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../src/osprey/packages/testing/src/check.py");
    let _ = std::fs::copy(check, directory.join("framework/check.py"))?;
    let sources = files
        .iter()
        .map(|(path, _)| format!("{path:?}"))
        .chain(std::iter::once("\"framework/check.py\"".to_owned()))
        .collect::<Vec<_>>()
        .join(",");
    let manifest = format!("[project]\nname = \"python-interop\"\nsource_roots = [\".\"]\npython_sources = [{sources}]\ndefault_namespace = \"fixture\"\nentry = \"main.py\"\n");
    std::fs::write(directory.join("osprey.toml"), manifest)?;
    for &(relative, source) in files {
        std::fs::write(directory.join(relative), source)?;
    }
    let result = ["gc", "arc"].into_iter().try_for_each(|memory| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"));
        let _ = command
            .arg(&directory)
            .args(["--run", "--quiet"])
            .arg(format!("--memory={memory}"));
        effect_execution::runtime_environment(&mut command, memory);
        let output = command.output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{name}/{memory}: {stderr}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{name}/{memory}"
        );
        if memory == "arc" {
            effect_execution::assert_arc_clean(&stderr);
        }
        Ok(())
    });
    let _ = std::fs::remove_dir_all(&directory);
    result
}

#[test]
fn python_modules_share_closures_results_and_lookups_across_the_project() {
    assert_project_output(
        "modules",
        &[
            ("framework/calc.py", PROJECT_CALC),
            ("framework/words.py", PROJECT_WORDS),
            ("main.py", PROJECT_MAIN),
        ],
        "7:ok:42:error:9\n\
         [rows=]:10:10\n\
         Success(a:b):Error(split: separator must not be empty):Success(10):Error(mapGet: key not found)\n",
    )
    .expect("python project runs under GC and ARC");
}

// Implements [PORT-STRING-NUL]: a text read must never claim a truncated success.
const NUL_FILE_READER: &str = r#"# osprey: flavor=python
def describe(path):
    match readFile(path):
        case Error(message=message):
            return "NUL" if contains(message, "NUL") else f"wrong-error:{message}"
        case Success(value=text):
            return f"value:{text}"

def main():
    return print(f"{describe(FILE_LEADING)}|{describe(FILE_MIDDLE)}|{describe(FILE_TRAILING)}|{describe(FILE_GROWN)}|{describe(FILE_PLAIN)}|{describe(FILE_EMPTY)}")
"#;

fn nul_read_source(directory: &std::path::Path) -> std::io::Result<String> {
    let grown = format!("{}\0suffix", "x".repeat(16_384));
    let files = [
        ("LEADING", "\0tail"),
        ("MIDDLE", "head\0tail"),
        ("TRAILING", "head\0"),
        ("GROWN", grown.as_str()),
        ("PLAIN", "héllo ☕"),
        ("EMPTY", ""),
    ];
    let mut source = NUL_FILE_READER.to_owned();
    for (name, bytes) in files {
        let path = directory.join(name);
        std::fs::write(&path, bytes)?;
        source = source.replace(
            &format!("FILE_{name}"),
            &format!("{:?}", path.to_string_lossy()),
        );
    }
    Ok(source)
}

fn assert_python_bytes(
    name: &str,
    source: &str,
    memory: &str,
    expected: &str,
) -> std::io::Result<()> {
    let output = effect_execution::execute(name, "py", source, memory)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{memory}: {stderr}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected,
        "{memory}: {stderr}"
    );
    if memory == "arc" {
        effect_execution::assert_arc_clean(&stderr);
    }
    Ok(())
}

fn assert_parallel_python_bytes(name: &str, source: &str, expected: &str) -> std::io::Result<()> {
    let results = std::thread::scope(|scope| {
        ["gc", "arc"]
            .map(|memory| scope.spawn(move || assert_python_bytes(name, source, memory, expected)))
            .map(std::thread::ScopedJoinHandle::join)
    });
    for result in results {
        result.map_err(|failure| {
            let message = failure
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| failure.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            std::io::Error::other(format!("{name} assertion failed: {message}"))
        })??;
    }
    Ok(())
}

#[test]
fn binary_nul_reads_fail_explicitly_instead_of_truncating() -> std::io::Result<()> {
    let directory = std::env::temp_dir().join(format!("osprey_nul_read_{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let source = nul_read_source(&directory)?;
    let result = assert_parallel_python_bytes(
        "nul_file_read",
        &source,
        "NUL|NUL|NUL|NUL|value:héllo ☕|value:\n",
    );
    std::fs::remove_dir_all(directory)?;
    result
}

// [PORT-STRING-NUL-CODEGEN] Use the private byte-counted runtime copy to
// test compiler consumers while public JSON/file NUL admission is guarded.
#[test]
#[cfg(target_pointer_width = "64")]
fn owned_nul_text_comparison_and_concat_preserve_all_bytes() -> std::io::Result<()> {
    let source = r#"# osprey: flavor=python
@extern
def osp_string_dup_internal(value: str, count: int) -> str:
    ...

def main():
    value = osp_string_dup_internal("head\0tail", 9)
    same = osp_string_dup_internal("head\0tail", 9)
    other = osp_string_dup_internal("head\0fail", 9)
    joined = value + "!"
    expected = osp_string_dup_internal("head\0tail!", 10)
    return print(f"{byteLength(value)}|{unwrap_or(byteAt(value, 4), -1)}|{value == 'head'}|{value == same}|{value == other}|{value > 'head'}|{value > other}|{listContains([value], 'head')}|{listContains([value], other)}|{listContains([value], same)}|{byteLength(joined)}|{unwrap_or(byteAt(joined, 4), -1)}|{unwrap_or(byteAt(joined, 9), -1)}|{joined == expected}")
"#;
    assert_parallel_python_bytes(
        "owned_nul_text",
        source,
        "9|0|false|true|false|true|true|false|false|true|10|0|33|true\n",
    )
}
