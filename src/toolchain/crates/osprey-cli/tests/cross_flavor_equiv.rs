//! Cross-flavor equivalence ([FLAVOR-CURRY], [FLAVOR-LAYER],
//! [FLAVOR-LOWER-CONTRACT],
//! docs/specs/language/0023-LanguageFlavors.md). ML **curries by default** and offers a
//! second *uncurried* surface, so it mirrors BOTH Default function forms at the
//! canonical AST (modulo source positions). Three buckets pin the boundary:
//!
//! 1. ML curried `add x y = …` (sugar for `add = \x -> \y -> …`) ≡ Default
//!    *explicit-curry* `fn add(x) = fn(y) => …` — a one-parameter
//!    [`Stmt::Function`] whose body is an [`Expr::Lambda`].
//! 2. ML uncurried `add (x, y) = …` ≡ Default *multi-parameter* `fn add(x, y)` —
//!    one flat two-parameter [`Stmt::Function`], the `(int, int) -> int` shape.
//! 3. The two are NOT interchangeable: ML curried `add x y` differs from Default
//!    multi-parameter `fn add(x, y)`. Currying is a real semantic difference, not
//!    a spelling — each ML surface form mirrors its own Default twin and no other.

use osprey_syntax::{parse_program_with_flavor, Flavor};

#[path = "common/effect_execution.rs"]
mod effect_execution;

/// `scripts/tests/check-python-gpt2.py` against the compiler under test with
/// `OSPREY_GPU=device`: every element of 24 tensors under both Tensor handlers
/// and both managed memory modes must match the independent reference.
fn assert_python_gpt2_matches(device: &str) -> std::io::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let python = if cfg!(windows) { "python" } else { "python3" };
    let output = std::process::Command::new(python)
        .arg(root.join("scripts/tests/check-python-gpt2.py"))
        .args([
            "--compiler",
            env!("CARGO_BIN_EXE_osprey"),
            "--device",
            device,
        ])
        .current_dir(root)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{device}: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("matched 24 complete tensors"),
        "{device}: {stdout}"
    );
    Ok(())
}

#[test]
fn python_gpt2_forward_tensors_match_the_independent_reference() -> std::io::Result<()> {
    assert_python_gpt2_matches("host")
}

/// [GPU-DEVICE], [GPU-BACKEND-DEVICE]: the same Python model body runs on the
/// GPU hardware this host has — Metal and `OpenCL` on macOS, the `OpenCL`
/// device `auto` resolves elsewhere. The check script demands a `metal:`/`opencl:`
/// identity that is not `unavailable` and completed dispatches in the trace, so
/// a run that fell back to the host can never pass. Without GPU hardware the
/// test is ignored: unverified delivery, never a green acceleration test.
#[test]
#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs GPU hardware: run with --ignored on a host that has a device"
)]
fn python_gpt2_device_runs_never_pass_on_host_fallback() -> std::io::Result<()> {
    let devices: &[&str] = if cfg!(target_os = "macos") {
        &["metal", "opencl"]
    } else {
        &["auto"]
    };
    devices
        .iter()
        .try_for_each(|device| assert_python_gpt2_matches(device))
}

const DEFAULT_GENERATOR: &str = "src/osprey/examples/llms/gpt2/generate";
const PYTHON_GENERATOR: &str = "src/osprey/examples/llms/gpt2/python/generate";

/// `osprey <project> --run --memory=arc` from the repository root, as `make
/// gpt2` runs it, with `stdin` and `OSPREY_GPU=device`.
fn generated(project: &str, stdin: &str, device: &str) -> std::io::Result<std::process::Output> {
    use std::io::Write;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(root.join(project))
        .args(["--run", "--memory=arc"])
        .env("OSPREY_GPU", device)
        .current_dir(&root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .map_or(Ok(()), |mut pipe| pipe.write_all(stdin.as_bytes()))?;
    child.wait_with_output()
}

/// Both generators exit cleanly on `stdin` and print the same bytes.
fn assert_generators_agree(stdin: &str, device: &str) -> std::io::Result<String> {
    let default = generated(DEFAULT_GENERATOR, stdin, device)?;
    let python = generated(PYTHON_GENERATOR, stdin, device)?;
    for (flavor, output) in [("default", &default), ("python", &python)] {
        assert!(
            output.status.success(),
            "{flavor} generator failed on {device}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        String::from_utf8_lossy(&python.stdout),
        String::from_utf8_lossy(&default.stdout),
        "python generator stdout differs from the default generator on {device}"
    );
    assert_eq!(
        python.stdout, default.stdout,
        "stdout bytes differ on {device}"
    );
    Ok(String::from_utf8_lossy(&python.stdout).into_owned())
}

/// [FLAVOR-PYTHON-GPU]: the Python-flavor generator is a self-contained port
/// of the Default generator, so its project holds no `.osp` source and reaches
/// none through a source root.
#[test]
fn python_gpt2_project_contains_no_osprey_flavor_source() -> std::io::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let python = root
        .join("src/osprey/examples/llms/gpt2/python")
        .canonicalize()?;
    let project = python.join("generate");
    let manifest = std::fs::read_to_string(project.join("osprey.toml"))?;
    let roots = manifest
        .lines()
        .find_map(|line| line.strip_prefix("source_roots = ["))
        .and_then(|rest| rest.strip_suffix(']'))
        .expect("the generator manifest declares its source roots");
    for source_root in roots.split(',').map(|path| path.trim().trim_matches('"')) {
        let resolved = project.join(source_root).canonicalize()?;
        assert!(
            resolved.starts_with(&python),
            "source root `{source_root}` reaches outside the Python port: {}",
            resolved.display()
        );
    }
    let mut pending = vec![python.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                assert!(
                    path.extension().is_none_or(|extension| extension != "osp"),
                    "{} is an Osprey-flavor source inside the Python port",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

/// The Python generator keeps the Default generator's stdin contract and
/// messages byte for byte. An unknown backend runs no model, so this path
/// needs no checkpoint and runs everywhere.
#[test]
fn python_gpt2_generator_prints_the_default_generator_messages() -> std::io::Result<()> {
    let stdout =
        assert_generators_agree("The capital of France is\nnot a count\nnowhere\n", "host")?;
    assert_eq!(stdout, "unknown backend `nowhere`: choose gpu or lists\n");
    Ok(())
}

/// The whole model in Python flavor: tokenizer, checkpoint loader, forward
/// pass, KV-cached steps and greedy sampling print the Default example's
/// output byte for byte on the host surface, over host lists and on the GPU
/// hardware this host has ([GPU-DEVICE], [FLAVOR-PYTHON-GPU]). It needs the
/// published checkpoint in src/osprey/examples/llms/gpt2/weights, which `make -C src/toolchain/targets gpt2` downloads.
#[test]
#[ignore = "needs the GPT-2 checkpoint (`make -C src/toolchain/targets gpt2` downloads it): run with --ignored"]
fn python_gpt2_generator_output_is_byte_identical_to_the_default_generator() -> std::io::Result<()>
{
    let devices: &[&str] = if cfg!(target_os = "macos") {
        &["host", "metal", "opencl"]
    } else {
        &["host", "auto"]
    };
    for device in devices {
        let stdout = assert_generators_agree("The capital of France is\n20\ngpu\n", device)?;
        assert!(
            stdout.starts_with(&format!("gpt2 (124M) on {device}")),
            "{device}: {stdout}"
        );
        assert!(
            stdout.contains("\ngenerated 20 tokens\n"),
            "{device}: {stdout}"
        );
    }
    let lists = assert_generators_agree("Melbourne is the capital of\n1\nlists\n", "host")?;
    assert!(lists.starts_with("gpt2 (124M) on lists"), "{lists}");
    Ok(())
}

/// The canonical AST as a debug string with every source `Position { … }`
/// payload scrubbed, so structural equality ignores spans.
fn canonical(src: &str, flavor: Flavor) -> String {
    let parsed = parse_program_with_flavor(src, flavor);
    assert!(
        parsed.errors.is_empty(),
        "unexpected {flavor} syntax errors: {:?}",
        parsed.errors
    );
    osprey_ast::canonical::without_positions(&parsed.program)
}

/// Default and ML spellings that must (or must not) lower to the same
/// canonical AST: `(default, ml, equal, why)`.
const CANONICAL_PAIRS: [(&str, &str, bool, &str); 4] = [
    // A `///`-documented Default function and its `(** … *)` ML twin lower to
    // the SAME DocComment: the doc body markup is flavor-neutral, only the
    // sigil differs ([DOC-MODEL], [DOC-ATTACH], [DOC-BODY-MARKDOWN],
    // [DOC-SECTIONS], [DOC-SIGIL-DEFAULT], [DOC-SIGIL-ML], [FLAVOR-BOUNDARY]).
    // Both are curried single-param functions so the whole AST matches.
    (
        "/// Doubles `x`.\n///\n/// # Returns\n/// twice x\nfn dbl(x) = fn(y) => x + y\n",
        "(** Doubles `x`.\n\n    # Returns\n    twice x *)\ndbl x y = x + y\n",
        true,
        "a documented function must lower to an identical DocComment in both flavors",
    ),
    // ML curries by default, so the multi-binding `add x y = …` IS the Default
    // explicit-curry chain: Function { params: [x] } returning Lambda { [y] }.
    (
        "fn add(x) = fn(y) => x + y\n",
        "add x y = x + y\n",
        true,
        "curried ML multi-binding must equal Default explicit-curry",
    ),
    // ML's uncurried surface `add (x, y) = …` is one flat two-parameter
    // Function, the mirror twin of every multi-parameter Default function.
    (
        "fn add(x, y) = x + y\n",
        "add (x, y) = x + y\n",
        true,
        "ML uncurried tuple binding must equal Default multi-param",
    ),
    // Currying is a real semantic difference, not a spelling: the curried ML
    // chain never lowers to the two-parameter Default form.
    (
        "fn add(x, y) = x + y\n",
        "add x y = x + y\n",
        false,
        "Default multi-param must NOT equal the curried ML multi-binding",
    ),
];

#[test]
fn flavor_spellings_lower_to_the_expected_canonical_ast() {
    for (default, ml, equal, why) in CANONICAL_PAIRS {
        let same = canonical(default, Flavor::Default) == canonical(ml, Flavor::Ml);
        assert_eq!(same, equal, "{why}:\n{default}\n{ml}");
    }
}

/// [TYPE-RECORD-UPDATE] Qualification and type application construct even when
/// the last segment is lowercase; `_` is a type name only in this context.
#[test]
fn record_heads_lower_identically_across_flavors() {
    for (head, variant) in [
        ("_", "TypeConstructor"),
        ("Ids::id", "TypeConstructor"),
        ("box<int>", "TypeConstructor"),
        ("Point", "TypeConstructor"),
        ("point", "Update"),
        ("_point", "Update"),
    ] {
        let default = canonical(&format!("let value = {head} {{ x: 1 }}\n"), Flavor::Default);
        let ml = canonical(&format!("value = {head}(x = 1)\n"), Flavor::Ml);
        assert!(default.contains(variant), "{head}: {default}");
        assert_eq!(default, ml, "record head {head}");
    }
}

/// [FLAVOR-PYTHON-TYPING] Inferred Python-flavor values use the same native ABI.
#[test]
fn python_hm_inference_executes_polymorphism_and_constrained_closures() {
    let source = r#"from typing import Callable
def identity(x):
    return x
def constly(value):
    return lambda ignored: value
held: Callable[[str], int] = constly(9)
def apply(callback: Callable[[str], int]):
    return callback("nested")
nested: Callable[[Callable[[str], int]], int] = apply
alias = identity
def parsed(text):
    match parseInt(text):
        case Success(value=value):
            return value
        case Error(message=message):
            return 0
def split_rows(text, separator):
    return split(text, separator)
def kept(text, separator):
    match split_rows(text, separator):
        case Success(value=parts):
            return Success(value=join(filterList(parts, lambda part: part != ""), ":"))
        case Error(message=message):
            return Error(message=message)
numbers = [alias(7), held("ignored")]
words = {"answer": alias("seven")}
print(f"{numbers[0]}:{numbers[1]}:{words["answer"]}:{identity(True)}:{parsed("42")}:{parsed("bad")}:{nested(held)}:{kept("a,,b", ",")}:{kept("x", "")}")
"#;
    effect_execution::assert_flavored_output(
        "python_hm",
        "py",
        source,
        "Success(7):Success(9):Success(seven):true:42:0:9:Success(a:b):Error(split: separator must not be empty)\n",
    );
}

const NAMED_PATTERN_ALIAS_PROGRAM: &str = r#"class Issue:
    issues: list[str]
def combine(first, rest):
    match rest:
        case Issue(issues=issues):
            match first:
                case Issue(issues=earlier):
                    return join(listConcat(earlier, issues), ":")
class Pair:
    first: str
    second: str
def siblings(items):
    match items:
        case [Issue(issues=left), Issue(issues=right)]:
            return join(listConcat(left, right), ":")
        case _:
            return "different"
def retained(issues, item):
    match item:
        case Issue(issues=alias):
            callback = lambda suffix: join(listConcat(listConcat(issues, alias), [suffix]), ":")
            return callback("tail")
def reordered(pair):
    match pair:
        case Pair(second=right, first=left):
            return left + ":" + right
def partial(pair):
    match pair:
        case Pair(second=right):
            return right
def result_alias(value, result):
    match result:
        case Success(value=inside):
            return value + ":" + inside
        case Error(message=inside):
            return value + ":error:" + inside
def ignored(item):
    match item:
        case Issue(issues=_):
            return "ignored"
def callable_packet(result):
    match result:
        case Success(value=message):
            return message("call")
        case Error(message=value):
            return len(value)
class Packet:
    payload: str
def packet(result):
    match result:
        case Success(value=message):
            return message.payload
        case Error(message=value):
            return value
def main():
    left = Issue(issues=["left"])
    right = Issue(issues=["right"])
    pair = Pair(first="first", second="second")
    print(f"{combine(left, right)}:{combine(right, left)}|{siblings([left, right])}|{siblings([right, left])}|{siblings([])}|{retained(["outer"], right)}|{reordered(pair)}|{partial(pair)}|{result_alias("outer", Success(value="inner"))}|{result_alias("outer", Error(message="bad"))}|{ignored(left)}|{packet(Success(value=Packet(payload="packet")))}:{packet(Error(message="refused"))}|{callable_packet(Success(value=lambda word: len(word)))}:{callable_packet(Error(message="denied"))}")
"#;

/// [FLAVOR-PYTHON-CLASS], [PATTERN-RESULT-BINDERS]: labels never bind hidden names.
#[test]
fn python_named_pattern_aliases_preserve_nested_and_sibling_values() {
    effect_execution::assert_flavored_output(
        "python_pattern_alias",
        "py",
        NAMED_PATTERN_ALIAS_PROGRAM,
        "left:right:right:left|left:right|right:left|different|outer:right:tail|first:second|second|outer:inner|outer:error:bad|ignored|packet:refused|4:6\n",
    );
    for (extension, source) in [
        (
            "osp",
            r#"type Packet = { payload: string }
fn packet(result) = match result {
    Success { message } => message.payload
    Error { value } => value
}
print(packet(Success { value: Packet { payload: "packet" } }) + ":" + packet(Error { message: "refused" }))
"#,
        ),
        (
            "ospml",
            r#"type Packet =
    payload : string
packet result = match result
    Success message => message.payload
    Error value => value
print (packet (Success(value = Packet(payload = "packet"))) + ":" + packet (Error(message = "refused")))
"#,
        ),
    ] {
        effect_execution::assert_flavored_output(
            "result_role",
            extension,
            source,
            "packet:refused\n",
        );
    }
    for (name, source, diagnostic) in [
        ("unknown_field", "class Item:\n    value: int\ndef read(item):\n    match item:\n        case Item(missing=other):\n            return other\nprint(read(Item(value=1)))\n", "unknown pattern field `missing`"),
        ("duplicate_field", "class Item:\n    value: int\ndef read(item):\n    match item:\n        case Item(value=left, value=right):\n            return left + right\nprint(read(Item(value=1)))\n", "duplicate pattern field `value`"),
        ("result_field", "def read(item):\n    match item:\n        case Success(message=other):\n            return other\n        case Error():\n            return 0\nprint(read(Success(value=1)))\n", "expected pattern field `value`"),
    ] {
        for memory in ["gc", "arc"] {
            let output = effect_execution::execute(name, "py", source, memory).expect("run rejected pattern");
            assert!(!output.status.success(), "{name}/{memory} accepted an invalid pattern");
            assert!(output.stdout.is_empty(), "{name}/{memory} ran invalid source");
            assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic), "{name}/{memory}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
