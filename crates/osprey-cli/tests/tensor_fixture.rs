//! Native end-to-end pins for the file-backed tensor loaders — the GPU
//! surface's [GPU-BUFFER-TENSOR] and [GPU-BUFFER-LOAD], the host's
//! [BUILTIN-FILE-TENSOR] — for [GPU-MATMUL] over a loaded tensor, and for the
//! separation of GPT-2's two `Tensor` handlers in both flavors.
//!
//! The corpus harness cannot read a committed file — wasm runs in a fresh
//! preopened sandbox and the mobile hosts chdir into a scratch directory — so
//! `fixtures/transformer.safetensors` is read here by an absolute path spliced
//! into both flavors of `fixtures/tensor_fixture`. The fixture is
//! `python3 -c "import json,struct;h={'w':{'dtype':'F32','shape':[2,2],'data_offsets':[0,16]},'h.1.b':{'dtype':'F16','shape':[2],'data_offsets':[16,20]}};j=json.dumps(h,separators=(',',':')).encode();open('transformer.safetensors','wb').write(struct.pack('<Q',len(j))+j+struct.pack('<4f',0.5,-1.0,2.0,3.0)+struct.pack('<2e',1.0,-2.5))"`:
//! a 116-byte header, so the data section starts at byte 124.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `w` is `[0.5, -1.0, 2.0, 3.0]` as a 2×2, `h.1.b` the binary16 pair, the
/// miss names the tensor exactly, `gpuLoad` at the data offset sees `w`'s
/// bytes, and `w · w` in k-order is `[-1.75, -3.5, 7.0, 7.0]`.
const EXPECTED: &str = "w 0.5 -1.0 2.0 3.0\n\
    bias 1.0 -2.5\n\
    missing gpuLoadTensor: h.11.b: no tensor of that name in the header\n\
    raw 0.5 -1.0 2.0 3.0\n\
    square -1.75 -3.5 7.0 7.0\n";

const TWINS: [(&str, &str); 2] = [
    ("osp", include_str!("fixtures/tensor_fixture.osp")),
    ("ospml", include_str!("fixtures/tensor_fixture.ospml")),
];

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Write `template` with the fixture's absolute path spliced in; a failed
/// write yields `None` and the test reports it through its own assertion.
fn program(ext: &str, template: &str) -> Option<PathBuf> {
    let fixture = manifest()
        .join("tests")
        .join("fixtures")
        .join("transformer.safetensors");
    let source = template.replace("@FIXTURE@", &fixture.display().to_string());
    let path = std::env::temp_dir().join(format!(
        "osprey_tensor_fixture_{}.{ext}",
        std::process::id()
    ));
    std::fs::write(&path, source).ok().map(|()| path)
}

/// `(exit code, stdout, stderr)` of `osprey <path> --run --quiet`, rooted at
/// the repo so the runtime archives resolve.
fn run(path: &Path) -> (Option<i32>, String, String) {
    run_with_gpu(path, "host")
}

fn run_with_gpu(path: &Path, backend: &str) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo())
        .arg(path)
        .args(["--run", "--quiet"])
        .env("OSPREY_GPU", backend)
        .output();
    match output {
        Ok(out) => (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ),
        Err(_) => (None, String::new(), String::new()),
    }
}

#[test]
fn both_flavors_read_the_safetensors_fixture_exactly() {
    for (ext, template) in TWINS {
        let path = program(ext, template);
        assert!(path.is_some(), "{ext}: the program could not be written");
        let (code, stdout, stderr) = path.as_deref().map(run).unwrap_or_default();
        assert_eq!(code, Some(0), "{ext}: stderr: {stderr}");
        assert_eq!(stdout, EXPECTED, "{ext}");
    }
}

fn repo() -> PathBuf {
    manifest().join("..").join("..")
}

/// `directory` holding `config` and the entry `entry`, whose `@FIXTURE@` names
/// the committed safetensors fixture.
fn project_at(directory: PathBuf, entry: &str, source: &str, config: &str) -> Option<PathBuf> {
    let fixture = manifest().join("tests/fixtures/transformer.safetensors");
    std::fs::create_dir_all(&directory).ok()?;
    std::fs::write(directory.join("osprey.toml"), config).ok()?;
    let source = source.replace("@FIXTURE@", &fixture.display().to_string());
    std::fs::write(directory.join(entry), source).ok()?;
    Some(directory)
}

/// A project whose `main.osp` is `source`, importing the gpt2 model modules.
fn gpt2_project(name: &str, source: &str) -> Option<PathBuf> {
    let directory = std::env::temp_dir().join(format!("osprey_gpt2_{name}_{}", std::process::id()));
    let model = repo().join("examples/gpt2/model");
    let model = serde_json::to_string(&model.to_string_lossy()).ok()?;
    let config = format!("[project]\nname = \"failure\"\nsource_roots = [{model}, \".\"]\ndefault_namespace = \"failure\"\nentry = \"main.osp\"\n");
    project_at(directory, "main.osp", source, &config)
}

/// A Python-flavor project whose `main.py` is `source`, over the port's own
/// modules. Python sources are manifest-relative, so it lives under target/.
fn python_gpt2_project(name: &str, source: &str) -> Option<PathBuf> {
    let directory = repo()
        .join("target")
        .join(format!("osprey_pygpt2_{name}_{}", std::process::id()));
    let port = repo().join("examples/gpt2/python");
    let port = serde_json::to_string(&port.to_string_lossy()).ok()?;
    let modules = ["backends", "cpu", "gpu", "model"]
        .map(|m| format!("\"../../examples/gpt2/python/{m}.py\""))
        .join(", ");
    let config = format!("[project]\nname = \"{name}\"\nsource_roots = [\".\", {port}]\npython_sources = [{modules}]\nentry = \"main.py\"\n");
    project_at(directory, "main.py", source, &config)
}

/// The `--llvm` text of a project; empty when it does not compile.
fn llvm(path: &Path) -> String {
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo())
        .arg(path)
        .arg("--llvm")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// One handler loads the fixture's `w`, squares it and names itself.
const DEFAULT_SQUARE: &str = "import gpt2::Backends\nimport gpt2::Model\nimport gpt2::@MODULE@\n\
fn square() = {\n    let w = Model::loaded(\"@FIXTURE@\", \"w\")\n    let p = Model::product(w, w, 2, 2, 2)\n    \
print(\"${perform Backends::Tensor.device()} ${perform Backends::Tensor.get(p, 0)} ${perform Backends::Tensor.get(p, 3)}\")\n}\n\
fn main() = match Backends::strictly(fn() => @MODULE@::@HANDLER@(square)) {\n    Success { value } => value\n    Error { message } => print(message)\n}\n";

const PYTHON_SQUARE: &str = "# osprey: flavor=python\nfrom gpt2 import Backends, Model, @MODULE@\nfrom gpt2.Backends import Tensor\n\n\n\
def square():\n    w = Model.loaded(\"@FIXTURE@\", \"w\")\n    p = Model.product(w, w, 2, 2, 2)\n    device = perform(Tensor.device())\n    \
first = perform(Tensor.get(p, 0))\n    last = perform(Tensor.get(p, 3))\n    return print(f\"{device} {first} {last}\")\n\n\n\
def main():\n    match Backends.strictly(lambda: @MODULE@.@HANDLER@(square)):\n        case Success(value=value):\n            return value\n        \
case Error(message=message):\n            return print(message)\n";

/// `square` under `module`'s handler in each flavor, as `(flavor, project)`.
fn square_projects(module: &str, handlers: [&str; 2]) -> Vec<(&'static str, Option<PathBuf>)> {
    let source = |template: &str, handler: &str| {
        template
            .replace("@MODULE@", module)
            .replace("@HANDLER@", handler)
    };
    vec![
        (
            "default",
            gpt2_project(
                &format!("{module}_osp"),
                &source(DEFAULT_SQUARE, handlers[0]),
            ),
        ),
        (
            "python",
            python_gpt2_project(&format!("{module}_py"), &source(PYTHON_SQUARE, handlers[1])),
        ),
    ]
}

/// [BUILTIN-FILE-TENSOR] The CPU handler loads and multiplies over host lists
/// with no GPU surface in the program at all: its IR names no `osprey_gpu_`
/// symbol, and it runs under an `OSPREY_GPU` that aborts any GPU call.
#[test]
fn the_cpu_handler_never_touches_the_gpu_in_either_flavor() {
    for (flavor, path) in square_projects("Cpu", ["onLists", "on_lists"]) {
        assert!(path.is_some(), "{flavor}: the program could not be written");
        let path = path.unwrap_or_default();
        let ir = llvm(&path);
        assert!(
            ir.contains("@osprey_load_tensor("),
            "{flavor}: no host load: {ir}"
        );
        assert!(
            !ir.contains("osprey_gpu_"),
            "{flavor}: the CPU handler reached the GPU surface: {ir}"
        );
        let (code, stdout, stderr) = run_with_gpu(&path, "invalid");
        assert_eq!(
            (code, stdout.as_str()),
            (Some(0), "lists -1.75 7.0\n"),
            "{flavor}: {stderr}"
        );
        let _ = std::fs::remove_dir_all(path);
    }
}

/// The GPU handler loads and multiplies through the GPU surface alone: its IR
/// names the host loader nowhere.
#[test]
fn the_gpu_handler_never_touches_the_host_tensor_surface_in_either_flavor() {
    for (flavor, path) in square_projects("Gpu", ["onGpu", "on_gpu"]) {
        assert!(path.is_some(), "{flavor}: the program could not be written");
        let path = path.unwrap_or_default();
        let ir = llvm(&path);
        assert!(
            ir.contains("@osprey_gpu_load_tensor(") && ir.contains("@osprey_gpu_matmul("),
            "{flavor}: {ir}"
        );
        assert!(!ir.contains("osprey_load_tensor"), "{flavor}: {ir}");
        let (code, stdout, stderr) = run_with_gpu(&path, "host");
        assert_eq!(
            (code, stdout.as_str()),
            (Some(0), "host -1.75 7.0\n"),
            "{flavor}: {stderr}"
        );
        let _ = std::fs::remove_dir_all(path);
    }
}

/// [GPU-BACKEND-DEVICE] A launch site whose kernel has no device form runs on
/// the CPU under the host and aborts under a device. No GPT-2 program has one,
/// so under a selected device every kernel the GPU handler launches runs there.
#[test]
fn every_gpt2_kernel_has_a_device_form() {
    for program in ["generate", "train", "python/generate"] {
        let ir = llvm(&repo().join("examples/gpt2").join(program));
        assert!(
            ir.contains("call i32 @osprey_gpu_launch("),
            "{program}: no device launch: {ir}"
        );
        assert!(
            !ir.contains("@osprey_gpu_declined"),
            "{program}: a kernel has no device form"
        );
    }
}

#[test]
fn gpt2_rejects_a_failed_gpu_dispatch_instead_of_generating_with_empty_matrices() {
    let source = "import gpt2::Backends\nimport gpt2::Model\nimport gpt2::Gpu\nfn main() = match Backends::strictly(fn() => Gpu::onGpu(fn() => print(gpuLength(Model::product(toGpu([1.0]), toGpu([1.0]), 1, 1, 1))))) {\n    Success { value } => value\n    Error { message } => print(message)\n}\n";
    let path = gpt2_project("failure", source);
    assert!(path.is_some(), "GPU failure project could not be written");
    let (code, stdout, stderr) = path
        .as_deref()
        .map(|p| run_with_gpu(p, "invalid"))
        .unwrap_or_default();
    // The failed product stops the run with the runtime's reason as an Error
    // ([FAILURE-EXPLICIT]); no length of an empty matrix is ever printed.
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.starts_with("gpu: device selection: unknown backend") && stdout.lines().count() == 1,
        "failed GPU dispatch continued: {stdout}"
    );
    if let Some(path) = path {
        let _ = std::fs::remove_dir_all(path);
    }
}

/// The arithmetic policy GPT-2's `main` installs halts on a fault and hands the
/// caller an `Error` naming it. Substituting `0`/`0.0` was indistinguishable
/// from a real logit, so a faulted run generated from fabricated values.
const ARITH_FAULTS: &str = r#"import gpt2::Backends

fn quotient(d) = 10 % d
fn ratio(d) = 3.0 / d
fn grow(n) = n * 4611686018427387904
fn report(outcome) = match outcome {
    Success { value } => "value ${value}"
    Error { message } => "fault: ${message}"
}

fn main() = print("${report(Backends::strictly(fn() => quotient(listLength([1, 2, 3]))))}\n${report(Backends::strictly(fn() => quotient(listLength([]))))}\n${report(Backends::strictly(fn() => ratio(toFloat(listLength([])))))}\n${report(Backends::strictly(fn() => grow(listLength([1, 2, 3]))))}")
"#;

#[test]
fn gpt2_arithmetic_faults_reach_the_caller_instead_of_substituting_zero() {
    let path = gpt2_project("arith", ARITH_FAULTS);
    assert!(path.is_some(), "Arith fault project could not be written");
    let (code, stdout, stderr) = path.as_deref().map(run).unwrap_or_default();
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(
        stdout,
        "value 1\n\
         fault: remainder by zero: 10 % 0\n\
         fault: division by zero: 3.0 / 0\n\
         fault: integer overflow: 3 * 4611686018427387904\n"
    );
    if let Some(path) = path {
        let _ = std::fs::remove_dir_all(path);
    }
}
