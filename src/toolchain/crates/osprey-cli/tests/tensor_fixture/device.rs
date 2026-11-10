//! Device-form pins: float32 accumulator width, GPT-2 handler projects in
//! both flavors, mapped file regions, binary bytes and arithmetic faults
//! ([GPU-DEVICE-NATIVE], [GPU-MATMUL]).
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{assert_stdout, manifest, preferred_backend, program, repo, run_plain, run_with_gpu};

/// [GPU-DEVICE-NATIVE] An open literal seed uses the inferred binary32 slot.
#[test]
fn float32_reductions_round_at_the_inferred_accumulator_width() {
    let source = r#"
fn add(a, b) = a + b
fn main() = match toGpu([toFloat32(16777216.0), toFloat32(1.0), toFloat32(-16777216.0)]) {
    Success { value } => {
        let xs = value
        match gpuFold(xs, 0.0, add) {
            Success { value } => print("${value} ${gpuGet(xs, 0) ?: toFloat32(99.0)}")
            Error { message } => print(message)
        }
    }
    Error { message } => print(message)
}
"#;
    let path = program("reduction32", "osp", source).expect("write reduction fixture");
    assert!(
        !llvm(&path).contains(GPU_DECLINED),
        "binary32 reduction must have a device form"
    );
    assert_stdout(&path, "host", "0.0 16777216.0\n");
    std::fs::remove_file(path).expect("remove reduction fixture");
}
/// [GPU-FOLD] Erased host metadata must retain the device body's numeric width.
#[test]
fn effect_handled_float32_maximum_keeps_its_device_result_width() {
    let source = "import gpt2::Backends\nimport gpt2::Gpu\nimport gpt2::Model\nfn work() = {\nlet values = Model::orStop(perform Backends::Tensor.fromList([toFloat32(-108.95427), toFloat32(1.25), toFloat32(-3.5)]), Model::empty)\nlet maximum = Model::peak(values)\nlet found = Model::indexOf(values, maximum)\nprint(\"${maximum} ${found}\")\n}\nfn main() = match Backends::strictly(fn() => Gpu::onGpu(work)) {\nSuccess { value } => value\nError { message } => print(message)\n}\n";
    let path = gpt2_project("maximum32", source);
    assert_stdout(&path, &preferred_backend(), "1.25 1\n");
    std::fs::remove_dir_all(path).expect("remove maximum fixture");
}
/// [GPU-SCAN] [GPU-FILTER] Both sequential shapes preserve binary32 elements.
#[test]
fn typed_scan_and_filter_keep_their_device_form_and_order() {
    let source = r#"
fn add(a, b) = a + b
fn main() = match toGpu([toFloat32(1.0), toFloat32(-2.0), toFloat32(3.0)]) {
    Success { value } => {
        let xs = value
        match gpuFilter(xs, fn(x) => x > 0.0) {
            Success { value } => {
                let positives = value
                match gpuScan(xs, 0.0, add) {
                    Success { value } => print("${gpuLength(positives)} ${gpuGet(positives, 0) ?: toFloat32(99.0)} ${gpuGet(positives, 1) ?: toFloat32(99.0)} ${gpuGet(value, 2) ?: toFloat32(99.0)}")
                    Error { message } => print(message)
                }
            }
            Error { message } => print(message)
        }
    }
    Error { message } => print(message)
}
"#;
    let path = program("sequence32", "osp", source).expect("write sequence fixture");
    assert!(
        !llvm(&path).contains(GPU_DECLINED),
        "scan and filter must have device forms"
    );
    assert_stdout(&path, &preferred_backend(), "2 1.0 3.0 2.0\n");
    std::fs::remove_file(path).expect("remove sequence fixture");
}
/// `directory` holding `config` and the entry `entry`, whose `@FIXTURE@` names
/// the committed safetensors fixture.
fn project_at(directory: PathBuf, entry: &str, source: &str, config: &str) -> PathBuf {
    let fixture = manifest().join("tests/fixtures/transformer.safetensors");
    let source = source.replace("@FIXTURE@", &fixture.display().to_string());
    let written = std::fs::create_dir_all(&directory)
        .and_then(|()| std::fs::write(directory.join("osprey.toml"), config))
        .and_then(|()| std::fs::write(directory.join(entry), source));
    assert!(
        written.is_ok(),
        "write project {}: {written:?}",
        directory.display()
    );
    directory
}
/// A project whose `main.osp` is `source`, importing the gpt2 model modules.
fn gpt2_project(name: &str, source: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("osprey_gpt2_{name}_{}", std::process::id()));
    let model = repo().join("src/osprey/examples/llms/gpt2/model");
    let model = serde_json::Value::from(model.to_string_lossy()).to_string();
    let config = format!("[project]\nname = \"failure\"\nsource_roots = [{model}, \".\"]\ndefault_namespace = \"failure\"\nentry = \"main.osp\"\n");
    project_at(directory, "main.osp", source, &config)
}
/// A Python-flavor project whose `main.py` is `source`, over the port's own
/// modules. Python sources are manifest-relative, so it lives under target/.
fn python_gpt2_project(name: &str, source: &str) -> PathBuf {
    let directory = repo()
        .join("target")
        .join(format!("osprey_pygpt2_{name}_{}", std::process::id()));
    let port = repo().join("src/osprey/examples/llms/gpt2/python");
    let port = serde_json::Value::from(port.to_string_lossy()).to_string();
    let modules = ["backends", "cpu", "gpu", "model"]
        .map(|m| format!("\"../../src/osprey/examples/llms/gpt2/python/{m}.py\""))
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
/// `(flavor, IR, stdout)` of one `square` handler run.
type SquareRun = (&'static str, String, String);
/// `square` under `module`'s handler in each flavor, as `(flavor, project)`.
fn square_projects(module: &str, handlers: [&str; 2]) -> Vec<(&'static str, PathBuf)> {
    let source = |template: &str, handler: &str| {
        let named = template.replace("@MODULE@", module);
        named.replace("@HANDLER@", handler)
    };
    let osp = format!("{module}_osp");
    let py = format!("{module}_py");
    let default = source(DEFAULT_SQUARE, handlers[0]);
    let python = source(PYTHON_SQUARE, handlers[1]);
    vec![
        ("default", gpt2_project(&osp, &default)),
        ("python", python_gpt2_project(&py, &python)),
    ]
}
/// `square` under `module`'s handler in each flavor, run on `backend`; every
/// project is removed again.
fn square_runs(module: &str, handlers: [&str; 2], backend: &str) -> Vec<SquareRun> {
    square_projects(module, handlers)
        .into_iter()
        .map(|(flavor, path)| {
            let ir = llvm(&path);
            let (code, stdout, stderr) = run_with_gpu(&path, backend);
            assert_eq!(code, Some(0), "{flavor}: {stderr}");
            let _ = std::fs::remove_dir_all(path);
            (flavor, ir, stdout)
        })
        .collect()
}
/// [LLM-WEIGHTS-MAPPED] A mapped region is the file's own bytes on the host and
/// on the GPU: `w` at byte 124, its square through the device product, single
/// bytes at `uint8`, and each refusal naming its reason.
#[test]
fn a_mapped_file_region_reads_the_same_on_host_and_device() -> Result<(), Box<dyn std::error::Error>>
{
    let path = manifest()
        .join("tests/fixtures/transformer.safetensors")
        .display()
        .to_string();
    let osp = "let nine: float32 = toFloat32(9.0)\nlet small: uint8 = 9\n\
fn wide(b: GpuBuffer<float32>) = print(\"${gpuLength(b)} ${gpuGet(b, 0) ?: nine} ${gpuGet(b, 3) ?: nine}\")\n\
fn region(offset: int, count: int) -> Result<GpuBuffer<float32>, Error> = gpuMapFile(\"@P@\", offset, count)\n\
fn bytes() -> Result<GpuBuffer<uint8>, Error> = gpuMapFile(\"@P@\", 0, 8)\n\
fn refused(read: Result<GpuBuffer<float32>, Error>) = match read {\n    Success { value } => wide(value)\n    Error { message } => print(message)\n}\n\
match region(124, 4) {\n    Success { value } => {\n        wide(value)\n        refused(gpuMatMul(value, value, 2, 2, 2))\n    }\n    Error { message } => print(message)\n}\n\
match bytes() {\n    Success { value } => print(\"${gpuLength(value)} ${gpuGet(value, 0) ?: small} ${gpuGet(value, 1) ?: small}\")\n    Error { message } => print(message)\n}\n\
refused(region(123, 1))\nrefused(region(124, 100000))\nrefused(region(-4, 1))\n"
        .replace("@P@", &path);
    let expected = format!(
        "4 0.5 3.0\n4 -1.75 7.0\n8 116 0\n\
         gpuMapFile: {path}: offset is not a multiple of the element width\n\
         gpuMapFile: {path}: the file ends before the region does\n\
         gpuMapFile: {path}: a negative offset\n"
    );
    let program = program("map", "osp", &osp).expect("write map fixture");
    // Metal exists only on macOS; elsewhere `auto` picks whatever device the runtime has.
    let devices: &[&str] = if cfg!(target_os = "macos") {
        &["host", "metal"]
    } else {
        &["host", "auto"]
    };
    for &gpu in devices {
        let output = run_plain(&program, Some(gpu));
        assert!(output.is_ok(), "{gpu}: not run");
        let output = output?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{gpu}: {stderr}");
        let unavailable = if cfg!(target_os = "linux") && gpu == "auto" {
            let errors = [
                "osprey_matmul32: OpenCL: OpenCL driver unavailable failed (OpenCL status -1)",
                "osprey_matmul32: OpenCL: GPU platform discovery failed (OpenCL status -1001)",
                "osprey_matmul32: OpenCL: GPU platform discovery failed (OpenCL status 0)",
                "osprey_matmul32: OpenCL: compute GPU discovery failed (OpenCL status 0)",
            ];
            stdout.lines().nth(1).filter(|line| errors.contains(line))
        } else {
            None
        };
        // Driverless Linux must return the typed error, while GPU-equipped hosts must
        // return the exact product and report the dispatch. Mapping assertions apply to both.
        let refused = unavailable.map(|error| expected.replacen("4 -1.75 7.0", error, 1));
        let reference = refused.as_deref().unwrap_or(&expected);
        assert_eq!(stdout, reference, "{gpu}: {stderr}");
        assert_eq!(stderr.contains("[gpu] metal:"), gpu == "metal", "{stderr}");
        if cfg!(target_os = "linux") {
            let opencl = stderr.contains("[gpu] opencl:");
            assert_eq!(opencl, gpu == "auto" && unavailable.is_none(), "{stderr}");
        }
    }
    std::fs::remove_file(&program)?;
    Ok(())
}
/// [BUILTIN-FILE-BYTES] Both flavors read binary through its NUL bytes: the
/// fixture's 8-byte header prefix is 116 then seven zeros, the F32 `0.5` at
/// byte 124 is `00 00 00 3f`, a span past the end stops at the end, and bad
/// numbers and a missing file are the operation's own errors.
#[test]
fn both_flavors_read_binary_bytes_through_every_nul() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = manifest().join("tests/fixtures/transformer.safetensors");
    let size = std::fs::metadata(&fixture)?.len();
    let path = fixture.display().to_string();
    let calls = format!(
        "show(\"head\", readFileBytes(\"{path}\", 0, 8))\n\
         show(\"half\", readFileBytes(\"{path}\", 124, 4))\n\
         show(\"tail\", readFileBytes(\"{path}\", {tail}, 100))\n\
         show(\"past\", readFileBytes(\"{path}\", {size}, 100))\n\
         show(\"negative\", readFileBytes(\"{path}\", -1, 1))\n\
         show(\"missing\", readFileBytes(\"/nonexistent/osprey.bin\", 0, 1))\n",
        tail = size - 2,
    );
    let osp = format!(
        "fn render(value: List<int>, i: int) -> string = match i >= listLength(value) {{\n    true => \"\"\n    false => \" ${{listGet(value, i) ?: -1}}${{render(value, satAdd(i, 1))}}\"\n}}\n\
         fn show(label: string, read: Result<List<int>, Error>) = match read {{\n    Success {{ value }} => print(\"${{label}} ${{listLength(value)}}:${{render(value, 0)}}\")\n    Error {{ message }} => print(\"${{label}} ${{message}}\")\n}}\n\
         {calls}"
    );
    let python = format!(
        "# osprey: flavor=python\n\n\ndef render(value: list[int], i: int) -> str:\n    if i >= listLength(value):\n        return \"\"\n    else:\n        return f\" {{unwrap_or(listGet(value, i), -1)}}{{render(value, satAdd(i, 1))}}\"\n\n\ndef show(label: str, read: Result[list[int], Error]):\n    match read:\n        case Success(value=value):\n            print(f\"{{label}} {{listLength(value)}}:{{render(value, 0)}}\")\n        case Error(message=message):\n            print(f\"{{label}} {{message}}\")\n\n\n\
         {calls}"
    );
    let bytes = std::fs::read(&fixture)?;
    let [first, second] = bytes
        .last_chunk::<2>()
        .ok_or("fixture needs two tail bytes")?;
    let tail = format!("{first} {second}");
    let expected = format!(
        "head 8: 116 0 0 0 0 0 0 0\nhalf 4: 0 0 0 63\ntail 2: {tail}\npast 0:\n\
         negative readFileBytes: {path}: a negative offset\n\
         missing readFileBytes: /nonexistent/osprey.bin: No such file or directory\n"
    );
    for (ext, source) in [("osp", osp), ("py", python)] {
        let path = program("read_bytes", ext, &source).expect("write read bytes fixture");
        let output = run_plain(&path, None);
        let _ = std::fs::remove_file(&path);
        assert!(output.is_ok(), "{ext}: not run");
        let output = output?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stdout, expected, "{ext}: {stderr}");
    }
    Ok(())
}
/// The host tensor loader the CPU handler must name.
const HOST_LOAD: &str = "@osprey_load_tensor(";
/// The device loader and product the GPU handler must name.
const GPU_LOAD: &str = "@osprey_gpu_load_tensor(";
const GPU_MATMUL: &str = "@osprey_gpu_matmul(";
/// [BUILTIN-FILE-TENSOR] The CPU handler loads and multiplies over host lists
/// with no GPU surface in the program at all: its IR names no `osprey_gpu_`
/// symbol, and it runs under an `OSPREY_GPU` that aborts any GPU call.
#[test]
fn the_cpu_handler_never_touches_the_gpu_in_either_flavor() {
    for (flavor, ir, stdout) in square_runs("Cpu", ["onLists", "on_lists"], "invalid") {
        assert!(ir.contains(HOST_LOAD), "{flavor}: no host load");
        assert!(!ir.contains("osprey_gpu_"), "{flavor}: reached GPU");
        assert_eq!(stdout, "lists -1.75 7.0\n", "{flavor}");
    }
}
/// The GPU handler loads and multiplies through the GPU surface alone: its IR
/// names the host loader nowhere.
#[test]
fn the_gpu_handler_never_touches_the_host_tensor_surface_in_either_flavor() {
    for (flavor, ir, stdout) in square_runs("Gpu", ["onGpu", "on_gpu"], "host") {
        assert!(ir.contains(GPU_LOAD), "{flavor}: no device load");
        assert!(ir.contains(GPU_MATMUL), "{flavor}: no device product");
        assert!(!ir.contains("osprey_load_tensor"), "{flavor}: host load");
        assert_eq!(stdout, "host -1.75 7.0\n", "{flavor}");
    }
}
/// The launcher every GPT-2 kernel must reach, and the decline marker.
const GPU_LAUNCH: &str = "call i32 @osprey_gpu_launch(";
const GPU_DECLINED: &str = "@osprey_gpu_declined";
/// [GPU-BACKEND-DEVICE] A launch site whose kernel has no device form runs on
/// the CPU under the host and aborts under a device. No GPT-2 program has one,
/// so under a selected device every kernel the GPU handler launches runs there.
#[test]
fn every_gpt2_kernel_has_a_device_form() {
    for program in ["generate", "train", "python/generate"] {
        let ir = llvm(&repo().join("src/osprey/examples/llms/gpt2").join(program));
        assert!(ir.contains(GPU_LAUNCH), "{program}: no device launch: {ir}");
        assert!(
            !ir.contains(GPU_DECLINED),
            "{program}: a kernel has no device form"
        );
    }
}
#[test]
fn gpt2_rejects_a_failed_gpu_dispatch_instead_of_generating_with_empty_matrices() {
    let source = r#"import gpt2::Backends
import gpt2::Model
import gpt2::Gpu
fn runProduct() = match toGpu([toFloat32(1.0)]) {
    Success { value } => print(gpuLength(Model::product(value, value, 1, 1, 1)))
    Error { message } => print("fixture upload failed: ${message}")
}
fn main() = match Backends::strictly(fn() => Gpu::onGpu(runProduct)) {
    Success { value } => value
    Error { message } => print(message)
}
"#;
    let path = gpt2_project("failure", source);
    let (code, stdout, stderr) = run_with_gpu(&path, "invalid");
    // The failed product stops the run with the runtime's reason as an Error
    // ([FAILURE-EXPLICIT]); no length of an empty matrix is ever printed.
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.starts_with("gpu: device selection: unknown backend") && stdout.lines().count() == 1,
        "failed GPU dispatch continued: {stdout}"
    );
    let _ = std::fs::remove_dir_all(path);
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
    let (code, stdout, stderr) = run_with_gpu(&path, "host");
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(
        stdout,
        "value 1\n\
         fault: remainder by zero: 10 % 0\n\
         fault: division by zero: 3.0 / 0\n\
         fault: integer overflow: 3 * 4611686018427387904\n"
    );
    let _ = std::fs::remove_dir_all(path);
}
