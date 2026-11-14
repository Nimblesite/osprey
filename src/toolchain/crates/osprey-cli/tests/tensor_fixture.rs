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
/// Write `template` with the fixture's absolute path spliced in, as `name.ext`;
/// a failed write yields `None` and the test reports it through its own assertion.
fn program(name: &str, ext: &str, template: &str) -> Option<PathBuf> {
    let fixture = manifest().join("tests/fixtures/transformer.safetensors");
    let source = template.replace("@FIXTURE@", &fixture.display().to_string());
    let file = format!("osprey_{name}_{}.{ext}", std::process::id());
    std::fs::write(std::env::temp_dir().join(&file), source)
        .ok()
        .map(|()| std::env::temp_dir().join(file))
}
/// `(exit code, stdout, stderr)` of `osprey <path> --run --quiet --memory=gc`,
/// rooted at the repo so the runtime archives resolve.
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
/// `OSPREY_GPU` when the harness sets one, else the host backend.
fn preferred_backend() -> String {
    std::env::var("OSPREY_GPU").unwrap_or_else(|_| "host".to_owned())
}
/// `osprey <path> --run`, with `OSPREY_GPU` set when `gpu` is given.
fn run_plain(path: &Path, gpu: Option<&str>) -> std::io::Result<std::process::Output> {
    let gpu_env = gpu.into_iter().map(|gpu| ("OSPREY_GPU", gpu));
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(path)
        .arg("--run")
        .envs(gpu_env)
        .output()
}
/// Run `path` with `backend` and assert it exits 0 with exactly `expected`.
fn assert_stdout(path: &Path, backend: &str, expected: &str) {
    let (code, stdout, stderr) = run_with_gpu(path, backend);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, expected, "{stderr}");
}
/// The runtime line a clean ARC shutdown prints exactly once.
const ARC_CLEAN: &str = "[osp-arc] exit: 0 live objects";
/// Run `path` under `gc` and `arc` for every `backend`, asserting exit 0 and
/// handing `(backend, memory, stdout, stderr)` to `check`; ARC exits cleanly.
fn for_each_launch(path: &Path, backends: &[&str], check: impl Fn(&str, &str, &str, &str)) {
    for memory in ["gc", "arc"] {
        for &backend in backends {
            let (code, stdout, stderr) = run_with_memory(path, backend, memory);
            assert_eq!(code, Some(0), "{backend} {memory}: {stderr}");
            check(backend, memory, &stdout, &stderr);
            if memory == "arc" {
                assert_eq!(stderr.matches(ARC_CLEAN).count(), 1, "{stderr}");
            }
        }
    }
}
#[test]
fn both_flavors_read_the_safetensors_fixture_exactly() {
    for (ext, template) in TWINS {
        let path = program("twin", ext, template).expect("write tensor fixture");
        assert_stdout(&path, "host", EXPECTED);
        std::fs::remove_file(path).expect("remove tensor fixture");
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
/// `fromGpu` refuses the NULL buffer, then lists round-trip length and head.
const READBACK_EXPECTED: &str = "fromGpu: buffer unavailable\n0:-1\n2:7\n";
#[test]
fn buffer_allocation_errors_reach_the_caller() {
    // [FAILURE-EXPLICIT] Invalid/OOM requests never become successful empties.
    for (ext, source) in [
        ("index_failures.osp", INDEX_FAILURE_SOURCE),
        ("index_failures.ospml", INDEX_FAILURE_ML),
    ] {
        let path = program(ext, ext, source).expect("write allocation fixture");
        for_each_launch(&path, &["host"], |_, memory, stdout, _| {
            assert_eq!(stdout, INDEX_FAILURE_EXPECTED, "{ext} {memory}");
        });
        std::fs::remove_file(path).expect("remove allocation fixture");
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
        let path = program(ext, ext, source).expect("write dispatch failure fixture");
        for_each_launch(&path, &["host", "invalid"], |backend, _, stdout, _| {
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
        });
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
        let path = program(ext, ext, source).expect("write readback fixture");
        for_each_launch(&path, &["host"], |_, memory, stdout, _| {
            assert_eq!(stdout, READBACK_EXPECTED, "{ext} {memory}");
        });
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
        let path = program("typed_sites", "osp", &source).expect("write typed tensor fixture");
        assert_stdout(&path, "host", "wide 4 0.5 3.0\nsmall 2 1.0 -2.5\n");
        std::fs::remove_file(path).expect("remove typed tensor fixture");
    }
}
fn repo() -> PathBuf {
    manifest().join("../../../..")
}
/// [GPU-BUFFER-ELEM]: actual compiled uint8 storage, not a reported width.
#[test]
fn a_million_uint8_elements_allocate_one_megabyte_and_drop_cleanly() {
    let source = r#"
extern fn osp_arc_live_bytes() -> int
effect UploadFailure { control stop: fn(string) -> GpuBuffer<uint8> }
fn upload(byte) = match range(0, 1000000) |> map(fn(_index) => byte) |> toGpu() {
    Success { value } => value
    Error { message } => perform UploadFailure.stop(toString(message))
}
fn main() = {
    handle UploadFailure { stop reason => print(reason) }
    let byte: uint8 = 127
    let before = osp_arc_live_bytes()
    let buffer = upload(byte)
    let used = wrapSub(osp_arc_live_bytes(), before)
    match gpuGet(buffer, 999999) {
        Success { value } => print("${used} ${gpuLength(buffer)} ${value}")
        Error { message } => print(toString(message))
    }
}
"#;
    for mode in ["extract", "inline"] {
        let path = program(&format!("dense_{mode}"), "osp", source).expect("write dense fixture");
        assert_dense_fixture(&path, mode);
        std::fs::remove_file(path).expect("remove dense fixture");
    }
}
/// Compile and run the dense fixture on `path` in `mode`, asserting that the
/// payload plus its 40-byte header allocated once and dropped cleanly.
fn assert_dense_fixture(path: &Path, mode: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo())
        .arg(path)
        .args(["--run", "--quiet", "--memory=arc", "--no-warnings"])
        .env("OSPREY_ARC_DEBUG", "1")
        .env("OSPREY_GPU_KERNELS", mode)
        .output();
    assert!(
        output.is_ok(),
        "{mode}: compile and run dense fixture: {output:?}"
    );
    let Ok(output) = output else { return };
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{mode}: {stderr}");
    assert_eq!(output.stdout, b"1000040 1000000 127\n", "{mode}: payload");
    assert!(stderr.contains(ARC_CLEAN), "{mode}: leaked");
}

#[path = "tensor_fixture/device.rs"]
mod device;

/// [FAILURE-EXPLICIT] A missing raw-ABI buffer is an error, not a valid empty.
#[test]
fn combinators_reject_missing_buffers_but_accept_empty_buffers() {
    for (ext, source, declaration, uploaded, missing) in [
        (
            "osp",
            DISPATCH_SOURCE,
            "extern fn osprey_gpu_alloc(count: int, bytes: int) -> GpuBuffer<int>\n",
            "toGpu([1, 2])",
            "Success { value: osprey_gpu_alloc(-1, 8) }",
        ),
        (
            "ospml",
            DISPATCH_ML,
            "extern osprey_gpu_alloc (count: int) (bytes: int) -> GpuBuffer<int>\n",
            "toGpu [1, 2]",
            "Success(value = osprey_gpu_alloc (-1) 8)",
        ),
    ] {
        for absent in [true, false] {
            let replacement = if absent {
                missing
            } else if ext == "osp" {
                "toGpu([])"
            } else {
                "toGpu []"
            };
            let fixture = format!("{declaration}{}", source.replace(uploaded, replacement));
            let path =
                program("missing_combinator", ext, &fixture).expect("write missing buffer fixture");
            for_each_launch(&path, &["host"], |_, memory, stdout, _| {
                let expected = if absent {
                    "GPU operation failed\n"
                } else {
                    "ok\n"
                };
                assert_eq!(stdout, expected.repeat(5), "{ext} {memory} absent={absent}");
            });
            std::fs::remove_file(path).expect("remove missing buffer fixture");
        }
    }
}

/// [GPU-KERNEL-EXTRACT] A depth cap is not proof of recursion; keep causes concise.
#[test]
fn device_inline_depth_diagnostics_preserve_distinct_helpers_without_repetition() {
    let chain = (1..27).fold("fn step0(x) = x\n".to_owned(), |source, i| {
        format!("{source}fn step{i}(x) = step{}(x)\n", i - 1)
    });
    for (helpers, entry) in [
        (
            "fn descend(x) = match x == 0 { true => x false => descend(wrapAdd(x, -1)) }\n"
                .to_owned(),
            "descend",
        ),
        (chain, "step26"),
    ] {
        let source = format!("{helpers}match toGpu([1]) {{ Success {{ value }} => match gpuMap(value, {entry}) {{ Success {{ value }} => print(gpuLength(value)) Error {{ message }} => print(message) }} Error {{ message }} => print(message) }}\n");
        let path = program("inline_depth", "osp", &source).expect("write depth fixture");
        let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
            .arg(&path)
            .args(["--llvm", "--quiet"])
            .output()
            .expect("compile depth fixture");
        std::fs::remove_file(path).expect("remove depth fixture");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = String::from_utf8_lossy(&output.stdout);
        assert!(ir.contains("call i32 @osprey_gpu_declined("), "{ir}");
        assert!(ir.contains("device inline depth limit"), "{ir}");
        assert!(!ir.contains("recurses"), "{ir}");
        if entry == "descend" {
            assert_eq!(ir.matches("descend: ").count(), 1, "{ir}");
        } else {
            assert!(ir.contains("step25: step24: "), "{ir}");
        }
    }
}
