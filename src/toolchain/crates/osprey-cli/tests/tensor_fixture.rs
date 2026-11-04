//! Native end-to-end pins for the file-backed tensor loaders — [GPU-BUFFER-TENSOR],
//! [GPU-BUFFER-LOAD], [BUILTIN-FILE-TENSOR], [GPU-MATMUL] and GPT-2's two handlers.
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
    run_with_memory(path, backend, "gc")
}
fn run_with_memory(path: &Path, backend: &str, memory: &str) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo())
        .arg(path)
        .args(["--run", "--quiet"])
        .arg(format!("--memory={memory}"))
        .env("OSPREY_ARC_DEBUG", "1")
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
const INDEX_FAILURE_SOURCE: &str = r#"
fn describe(result: Result<GpuBuffer<int>, Error>) = match result {
    Success { value } => "length ${gpuLength(value)}"
    Error { message } => toString(message)
}
fn copied(xs) = toGpu(xs)
print("${describe(gpuIota(0))}\n${describe(gpuIota(3))}\n${describe(gpuIota(-1))}\n${describe(gpuIota(9223372036854775807))}\n${describe(gpuIota(1152921504606846975))}\n${describe(toGpu(range(0, 9223372036854775807)))}\n${describe(toGpu(range(-9223372036854775807, 9223372036854775807)))}\n${describe(toGpu(range(3, 0)))}\n${describe(toGpu(range(-2, 2)))}\n${describe(toGpu([7, 8]))}\n${describe(copied([4, 5, 6]))}\n${describe(toGpu([]))}\n${describe(copied([]))}\n${describe(toGpu(range(9223372036854775807, -9223372036854775807)))}")
"#;
const INDEX_FAILURE_ML: &str = r#"
describe : Result<GpuBuffer<int>, Error> -> string
describe result = match result
    Success value => "length ${gpuLength value}"
    Error message => toString message
copied xs = toGpu xs
print "${describe (gpuIota 0)}\n${describe (gpuIota 3)}\n${describe (gpuIota (-1))}\n${describe (gpuIota 9223372036854775807)}\n${describe (gpuIota 1152921504606846975)}\n${describe (toGpu (range (0, 9223372036854775807)))}\n${describe (toGpu (range (-9223372036854775807, 9223372036854775807)))}\n${describe (toGpu (range (3, 0)))}\n${describe (toGpu (range (-2, 2)))}\n${describe (toGpu [7, 8])}\n${describe (copied [4, 5, 6])}\n${describe (toGpu [])}\n${describe (copied [])}\n${describe (toGpu (range (9223372036854775807, -9223372036854775807)))}"
"#;
const INDEX_FAILURE_EXPECTED: &str = "length 0\nlength 3\ngpu: buffer allocation: a negative element count\ngpu: buffer allocation: element count overflows the byte size\ngpu: buffer allocation: out of memory\ngpu: buffer allocation: element count overflows the byte size\ntoGpu: range span overflows the element count\nlength 0\nlength 4\nlength 2\nlength 3\nlength 0\nlength 0\nlength 0\n";
#[test]
fn buffer_allocation_errors_reach_the_caller() {
    // [FAILURE-EXPLICIT] Invalid/OOM requests never become successful empties.
    for (ext, source) in [
        ("index_failures.osp", INDEX_FAILURE_SOURCE),
        ("index_failures.ospml", INDEX_FAILURE_ML),
    ] {
        let path = program(ext, source).expect("write index allocation fixture");
        for memory in ["gc", "arc"] {
            let (code, stdout, stderr) = run_with_memory(&path, "host", memory);
            assert_eq!(code, Some(0), "{ext} {memory}: {stderr}");
            assert_eq!(stdout, INDEX_FAILURE_EXPECTED, "{ext} {memory}");
            if memory == "arc" {
                assert_eq!(
                    stderr.matches("[osp-arc] exit: 0 live objects").count(),
                    1,
                    "{stderr}"
                );
            }
        }
        std::fs::remove_file(path).expect("remove index allocation fixture");
    }
}
const DISPATCH_SOURCE: &str = "fn report(result) = match result { Success _ => \"ok\" Error { message } => message }\nmatch toGpu([1, 2]) { Success { value } => {\nlet input = value\nprint(\"${report(gpuMap(input, fn(x) => wrapAdd(x, 1)))}\\n${report(gpuFold(input, 0, wrapAdd))}\\n${report(gpuZipWith(input, input, wrapAdd))}\\n${report(gpuScan(input, 0, wrapAdd))}\\n${report(gpuFilter(input, fn(x) => x == 1))}\")\n} Error { message } => print(message) }\n";
const DISPATCH_ML: &str = "report result = match result\n    Success _ => \"ok\"\n    Error message => message\nrun () = match toGpu [1, 2]\n    Success value =>\n        input = value\n        print \"${report (gpuMap (input, \\x => wrapAdd (x, 1)))}\\n${report (gpuFold (input, 0, wrapAdd))}\\n${report (gpuZipWith (input, input, wrapAdd))}\\n${report (gpuScan (input, 0, wrapAdd))}\\n${report (gpuFilter (input, \\x => x == 1))}\"\n    Error message => print message\nrun ()\n";
/// [FAILURE-EXPLICIT] All launch forms return Error before exposing output;
/// the same kernels succeed on the host, under both managed memory backends.
#[test]
fn every_combinator_preserves_the_dispatch_failure_reason() {
    for (ext, source) in [
        ("dispatch.osp", DISPATCH_SOURCE),
        ("dispatch.ospml", DISPATCH_ML),
    ] {
        let path = program(ext, source).expect("write dispatch failure fixture");
        for memory in ["gc", "arc"] {
            for backend in ["host", "invalid"] {
                let (code, stdout, stderr) = run_with_memory(&path, backend, memory);
                assert_eq!(code, Some(0), "{ext} {memory} {backend}: {stderr}");
                let lines: Vec<_> = stdout.lines().collect();
                assert_eq!(lines.len(), 5, "{stdout}");
                if backend == "host" {
                    assert!(lines.iter().all(|line| *line == "ok"), "{stdout}");
                } else {
                    assert!(
                        lines.iter().all(|line| line
                            .contains("unknown backend; expected host, auto, metal, or opencl")),
                        "{stdout}"
                    );
                }
                if memory == "arc" {
                    assert_eq!(
                        stderr.matches("[osp-arc] exit: 0 live objects").count(),
                        1,
                        "{stderr}"
                    );
                }
            }
        }
        std::fs::remove_file(path).expect("remove dispatch failure fixture");
    }
}
const READBACK_SOURCE: &str = "\nextern fn osprey_gpu_alloc(count: int, bytes: int) -> GpuBuffer<int>\nfn describe(buffer) = match fromGpu(buffer) {\n    Success { value } => \"${listLength(value)}:${listGet(value, 0) ?: -1}\"\n    Error { message } => toString(message)\n}\nfn uploaded(result) = match result { Success { value } => describe(value) Error { message } => toString(message) }\nprint(\"${describe(osprey_gpu_alloc(-1, 8))}\\n${uploaded(toGpu([]))}\\n${uploaded(toGpu([7, 8]))}\")\n";
const READBACK_ML: &str = "\nextern osprey_gpu_alloc (count: int) (bytes: int) -> GpuBuffer<int>\ndescribe buffer = match fromGpu buffer\n    Success value => \"${listLength value}:${listGet (value, 0) ?: -1}\"\n    Error message => toString message\nuploaded result = match result\n    Success value => describe value\n    Error message => toString message\nprint \"${describe (osprey_gpu_alloc (-1) 8)}\\n${uploaded (toGpu [])}\\n${uploaded (toGpu [7, 8])}\"\n";
#[test]
fn readback_preserves_lists_and_refuses_a_missing_buffer() {
    // [FAILURE-EXPLICIT] The raw ABI makes NULL reachable without faking OOM.
    for (ext, source) in [
        ("readback.osp", READBACK_SOURCE),
        ("readback.ospml", READBACK_ML),
    ] {
        let path = program(ext, source).expect("write readback fixture");
        for memory in ["gc", "arc"] {
            let (code, stdout, stderr) = run_with_memory(&path, "host", memory);
            assert_eq!(code, Some(0), "{ext} {memory}: {stderr}");
            assert_eq!(stdout, "fromGpu: buffer unavailable\n0:-1\n2:7\n");
            if memory == "arc" {
                assert!(
                    stderr.contains("[osp-arc] exit: 0 live objects"),
                    "{stderr}"
                );
            }
        }
        std::fs::remove_file(path).expect("remove readback fixture");
    }
}
#[test]
fn tensor_loader_calls_in_one_record_keep_distinct_requested_element_types() {
    // [GPU-BUFFER-TENSOR] Both calls share the record's source position, not its type.
    let source = r#"
type Loaded = { wide: Result<GpuBuffer<float32>, Error>, small: Result<GpuBuffer<float16>, Error> }
fn loaded() = Loaded {
    wide: gpuLoadTensor("@FIXTURE@", "w"),
    small: gpuLoadTensor("@FIXTURE@", "h.1.b")
}
fn main() = {
    let pair = loaded()
    let wide = match pair.wide {
        Success { value } => "wide ${gpuLength(value)} ${gpuGet(value, 0) ?: toFloat32(0.0)} ${gpuGet(value, 3) ?: toFloat32(0.0)}"
        Error { message } => "wrong wide: ${message}"
    }
    let small = match pair.small {
        Success { value } => "small ${gpuLength(value)} ${gpuGet(value, 0) ?: toFloat16(0.0)} ${gpuGet(value, 1) ?: toFloat16(0.0)}"
        Error { message } => "wrong small: ${message}"
    }
    print("${wide}\n${small}")
}
"#;
    for prefix in [
        "",
        "let load = gpuLoadTensor\n",
        "fn load(path, name) = gpuLoadTensor(path, name)\n",
    ] {
        let body = if prefix.is_empty() {
            source.to_owned()
        } else {
            source.replace("gpuLoadTensor(", "load(")
        };
        let source = format!("{prefix}{body}");
        let path = program("typed_sites.osp", &source).expect("write typed tensor fixture");
        let (code, stdout, stderr) = run(&path);
        assert_eq!(code, Some(0), "{prefix}: {stderr}");
        assert_eq!(
            stdout, "wide 4 0.5 3.0\nsmall 2 1.0 -2.5\n",
            "{prefix}: {stderr}"
        );
        std::fs::remove_file(path).expect("remove typed tensor fixture");
    }
}
fn repo() -> PathBuf {
    manifest().join("../../../..")
}
/// [GPU-DEVICE-NATIVE] An open literal seed uses the inferred binary32 slot.
#[test]
fn float32_reductions_round_at_the_inferred_accumulator_width() {
    let source = "fn add(a, b) = a + b\nfn main() = {\nlet xs = toGpu([toFloat32(16777216.0), toFloat32(1.0), toFloat32(-16777216.0)])\nprint(\"${gpuFold(xs, 0.0, add)} ${gpuGet(xs, 0) ?: toFloat32(99.0)}\")\n}\n";
    let path = program("reduction32.osp", source).expect("write reduction fixture");
    assert!(
        !llvm(&path).contains("@osprey_gpu_declined"),
        "binary32 reduction must have a device form"
    );
    let (code, stdout, stderr) = run(&path);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "0.0 16777216.0\n", "{stderr}");
    std::fs::remove_file(path).expect("remove reduction fixture");
}
/// [GPU-FOLD] Erased host metadata must retain the device body's numeric width.
#[test]
fn effect_handled_float32_maximum_keeps_its_device_result_width() {
    let source = "import gpt2::Backends\nimport gpt2::Gpu\nimport gpt2::Model\nfn work() = {\nlet values = perform Backends::Tensor.fromList([toFloat32(-108.95427), toFloat32(1.25), toFloat32(-3.5)])\nlet maximum = perform Backends::Tensor.reduce(values, Model::at(values, 0), Model::maxF)\nlet found = Model::indexOf(values, maximum)\nprint(\"${maximum} ${found}\")\n}\nfn main() = match Backends::strictly(fn() => Gpu::onGpu(work)) {\nSuccess { value } => value\nError { message } => print(message)\n}\n";
    let path = gpt2_project("maximum32", source).expect("write float32 maximum fixture");
    let backend = std::env::var("OSPREY_GPU").unwrap_or_else(|_| "host".to_owned());
    let (code, stdout, stderr) = run_with_gpu(&path, &backend);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "1.25 1\n", "{stderr}");
    std::fs::remove_dir_all(path).expect("remove maximum fixture");
}
/// [GPU-SCAN] [GPU-FILTER] Both sequential shapes preserve binary32 elements.
#[test]
fn typed_scan_and_filter_keep_their_device_form_and_order() {
    let source = "fn add(a, b) = a + b\nfn main() = {\nlet xs = toGpu([toFloat32(1.0), toFloat32(-2.0), toFloat32(3.0)])\nlet positives = gpuFilter(xs, fn(x) => x > 0.0)\nlet scanned = gpuScan(xs, 0.0, add)\nprint(\"${gpuLength(positives)} ${gpuGet(positives, 0) ?: toFloat32(99.0)} ${gpuGet(positives, 1) ?: toFloat32(99.0)} ${gpuGet(scanned, 2) ?: toFloat32(99.0)}\")\n}\n";
    let path = program("sequence32.osp", source).expect("write sequence fixture");
    assert!(
        !llvm(&path).contains("@osprey_gpu_declined"),
        "scan and filter must have device forms"
    );
    let backend = std::env::var("OSPREY_GPU").unwrap_or_else(|_| "host".to_owned());
    let (code, stdout, stderr) = run_with_gpu(&path, &backend);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "2 1.0 3.0 2.0\n", "{stderr}");
    std::fs::remove_file(path).expect("remove sequence fixture");
}
/// [GPU-BUFFER-ELEM]: actual compiled uint8 storage, not a reported width.
#[test]
fn a_million_uint8_elements_allocate_one_megabyte_and_drop_cleanly() {
    let source = "extern fn osp_arc_live_bytes() -> int\nfn main() = {\nlet byte: uint8 = 127\nlet before = osp_arc_live_bytes()\nlet buffer = range(0, 1000000) |> map(fn(_index) => byte) |> toGpu()\nlet used = wrapSub(osp_arc_live_bytes(), before)\nprint(\"${used} ${gpuLength(buffer)} ${gpuGet(buffer, 999999) ?: byte}\")\n}\n";
    for mode in ["extract", "inline"] {
        let path = std::env::temp_dir().join(format!(
            "osprey_dense_uint8_{}_{mode}.osp",
            std::process::id()
        ));
        std::fs::write(&path, source).expect("write dense buffer fixture");
        let output = dense_fixture_output(&path, mode).expect("compile and run dense fixture");
        assert_dense_fixture(&output, mode);
        std::fs::remove_file(path).expect("remove dense fixture");
    }
}
fn dense_fixture_output(path: &Path, mode: &str) -> std::io::Result<std::process::Output> {
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo())
        .arg(path)
        .args(["--run", "--quiet", "--memory=arc", "--no-warnings"])
        .env("OSPREY_ARC_DEBUG", "1")
        .env("OSPREY_GPU_KERNELS", mode)
        .output()
}
fn assert_dense_fixture(output: &std::process::Output, mode: &str) {
    assert!(
        output.status.success(),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout, b"1000040 1000000 127\n",
        "{mode}: payload + 40-byte header"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "{mode}: leaked"
    );
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
    let model = repo().join("src/osprey/examples/llms/gpt2/model");
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
    let port = repo().join("src/osprey/examples/llms/gpt2/python");
    let port = serde_json::to_string(&port.to_string_lossy()).ok()?;
    let modules = ["backends", "cpu", "gpu", "model"]
        .map(|m| format!("\"../../../osprey/examples/llms/gpt2/python/{m}.py\""))
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
    let program = std::env::temp_dir().join(format!("osprey_map_{}.osp", std::process::id()));
    assert!(std::fs::write(&program, osp).is_ok(), "not written");
    for gpu in ["host", "metal"] {
        let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
            .arg(&program)
            .arg("--run")
            .env("OSPREY_GPU", gpu)
            .output();
        assert!(output.is_ok(), "{gpu}: not run");
        let output = output?;
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{gpu}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reported = String::from_utf8_lossy(&output.stderr).contains("[gpu] metal:");
        assert_eq!(reported, gpu == "metal", "{gpu}: where the product ran");
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
    let osp = format!(
        "fn render(value: List<int>, i: int) -> string = match i >= listLength(value) {{\n    true => \"\"\n    false => \" ${{listGet(value, i) ?: -1}}${{render(value, satAdd(i, 1))}}\"\n}}\n\
         fn show(label: string, read: Result<List<int>, Error>) = match read {{\n    Success {{ value }} => print(\"${{label}} ${{listLength(value)}}:${{render(value, 0)}}\")\n    Error {{ message }} => print(\"${{label}} ${{message}}\")\n}}\n\
         show(\"head\", readFileBytes(\"{path}\", 0, 8))\n\
         show(\"half\", readFileBytes(\"{path}\", 124, 4))\n\
         show(\"tail\", readFileBytes(\"{path}\", {tail}, 100))\n\
         show(\"past\", readFileBytes(\"{path}\", {size}, 100))\n\
         show(\"negative\", readFileBytes(\"{path}\", -1, 1))\n\
         show(\"missing\", readFileBytes(\"/nonexistent/osprey.bin\", 0, 1))\n",
        tail = size - 2,
    );
    let python = format!(
        "# osprey: flavor=python\n\n\ndef render(value: list[int], i: int) -> str:\n    if i >= listLength(value):\n        return \"\"\n    else:\n        return f\" {{unwrap_or(listGet(value, i), -1)}}{{render(value, satAdd(i, 1))}}\"\n\n\ndef show(label: str, read: Result[list[int], Error]):\n    match read:\n        case Success(value=value):\n            print(f\"{{label}} {{listLength(value)}}:{{render(value, 0)}}\")\n        case Error(message=message):\n            print(f\"{{label}} {{message}}\")\n\n\n\
         show(\"head\", readFileBytes(\"{path}\", 0, 8))\n\
         show(\"half\", readFileBytes(\"{path}\", 124, 4))\n\
         show(\"tail\", readFileBytes(\"{path}\", {tail}, 100))\n\
         show(\"past\", readFileBytes(\"{path}\", {size}, 100))\n\
         show(\"negative\", readFileBytes(\"{path}\", -1, 1))\n\
         show(\"missing\", readFileBytes(\"/nonexistent/osprey.bin\", 0, 1))\n",
        tail = size - 2,
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
        let program =
            std::env::temp_dir().join(format!("osprey_read_bytes_{}.{ext}", std::process::id()));
        assert!(
            std::fs::write(&program, source).is_ok(),
            "{ext}: not written"
        );
        let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
            .arg(&program)
            .args(["--run"])
            .output();
        let _ = std::fs::remove_file(&program);
        assert!(output.is_ok(), "{ext}: not run");
        let output = output?;
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{ext}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
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
        let ir = llvm(&repo().join("src/osprey/examples/llms/gpt2").join(program));
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
    let source = "import gpt2::Backends\nimport gpt2::Model\nimport gpt2::Gpu\nfn main() = match Backends::strictly(fn() => Gpu::onGpu(fn() => print(gpuLength(Model::product(toGpu([toFloat32(1.0)]), toGpu([toFloat32(1.0)]), 1, 1, 1))))) {\n    Success { value } => value\n    Error { message } => print(message)\n}\n";
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
