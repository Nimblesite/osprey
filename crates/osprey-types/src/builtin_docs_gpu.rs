//! Canonical GPU builtin prose and examples. Types come from builtins.rs.
use crate::builtin_docs::BuiltinDoc;
use crate::builtin_docs_lang::builtin_doc;

/// GPU computation built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
///
/// GPU surface from docs/specs/runtime/0034-GPUComputation.md: [GPU-BUFFER-FROM-LIST],
/// [GPU-BUFFER-TO-LIST], [GPU-BUFFER-LENGTH], [GPU-MAP], [GPU-FOLD],
/// [GPU-ZIPWITH], [GPU-IOTA], [GPU-GET], [GPU-SCAN], [GPU-FILTER],
/// [GPU-DEVICE].
pub(crate) static GPU: &[BuiltinDoc] = &[
    builtin_doc!(
        "toGpu",
        "Copies a list of scalars (int, float, or bool) into a dense GpuBuffer. Also consumes an iterator pipeline, replaying its map/filter stages into the buffer with no list in between.",
        ["source" => "The scalar list or iterator to copy into a buffer"],
        "let buf = toGpu([1, 2, 3, 4])",
    ),
    builtin_doc!(
        "fromGpu",
        "Materializes a GpuBuffer back into a host list, keeping the element type: a float buffer becomes a `List<float>`.",
        ["buffer" => "The buffer to copy back to a list"],
        "fromGpu(toGpu([1, 2])) |> forEachList(print)  // Prints: 1, 2",
    ),
    builtin_doc!(
        "gpuLength",
        "Returns a GpuBuffer's element count. O(1).",
        ["buffer" => "The buffer to measure"],
        "print(gpuLength(toGpu([1, 2, 3])))  // 3",
    ),
    builtin_doc!(
        "gpuMap",
        "Applies a pure kernel to every buffer element independently, one thread per element when OSPREY_GPU selects Metal or OpenCL. The compiler rejects a kernel that performs any effect a handler must discharge at runtime; an effect a static handler has already erased is gone before the check.",
        ["buffer" => "The source buffer", "kernel" => "The pure per-element function"],
        "let squared = toGpu([1, 2, 3]) |> gpuMap(fn(x) => wrapMul(x, x))\nprint(gpuGet(squared, 2) ?: 0)  // 9",
    ),
    builtin_doc!(
        "gpuFold",
        "Reduces a buffer to one scalar with a pure combine function, applied left to right on every backend. A device runs the fold on one thread, so fold a large buffer by rows and then fold the rows.",
        ["buffer" => "The buffer to reduce", "initial" => "The initial scalar accumulator", "combine" => "The pure (accumulator, element) function"],
        "print(toGpu([1, 2, 3]) |> gpuFold(0, wrapAdd))  // 6",
    ),
    builtin_doc!(
        "gpuZipWith",
        "Combines two buffers elementwise with a pure binary kernel. The result takes the shorter operand's length.",
        ["a" => "The left buffer", "b" => "The right buffer", "kernel" => "The pure (a, b) element function"],
        "let products = gpuZipWith(toGpu([2, 3]), toGpu([4, 5]), wrapMul)\nprint(gpuGet(products, 1) ?: 0)  // 15",
    ),
    builtin_doc!(
        "gpuIota",
        "Builds the index buffer [0, n). Gather, stencil, and matrix addressing start here.",
        ["n" => "The element count"],
        "let indices = gpuIota(4) |> fromGpu()\nforEachList(indices, print)  // 0, 1, 2, 3",
    ),
    builtin_doc!(
        "gpuGet",
        "Bounds-checked read of one element at the buffer's element type. Out of bounds returns Error.",
        ["buffer" => "The buffer to read", "index" => "The element index"],
        "match gpuGet(toGpu([10, 20]), 1) {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuScan",
        "Inclusive prefix scan: element i is combine folded left to right through element i. It has no device form yet: when OSPREY_GPU selects a device, the run stops and names it.",
        ["buffer" => "The buffer to scan", "initial" => "The initial scalar accumulator", "combine" => "The pure (accumulator, element) function"],
        "// Run with OSPREY_GPU=host; scan has no device form yet.\nlet sums = toGpu([1, 2, 3]) |> gpuScan(0, wrapAdd)\nforEachList(fromGpu(sums), print)  // 1, 3, 6",
    ),
    builtin_doc!(
        "gpuFilter",
        "Stream compaction: keeps the elements a pure predicate accepts, preserving order. It has no device form yet: when OSPREY_GPU selects a device, the run stops and names it.",
        ["buffer" => "The source buffer", "predicate" => "The pure element predicate"],
        "// Run with OSPREY_GPU=host; filter has no device form yet.\nlet evens = toGpu([1, 2, 3, 4]) |> gpuFilter(fn(x) => x % 2 == 0)\nforEachList(fromGpu(evens), print)  // 2, 4",
    ),
    builtin_doc!(
        "gpuMatMul",
        "The dense row-major product of a rows x inner float buffer and an inner x cols one, as a rows x cols buffer. Every element accumulates in k order from 0.0 in double; a length that disagrees with the stated shape is an Error naming the operand.",
        ["left" => "The rows x inner buffer", "right" => "The inner x cols buffer", "rows" => "The left operand's row count", "inner" => "The shared dimension", "cols" => "The right operand's column count"],
        "let left = toGpu([1.0, 2.0, 3.0, 4.0])\nlet right = toGpu([5.0, 6.0, 7.0, 8.0])\nmatch gpuMatMul(left, right, 2, 2, 2) {\n    Success { value } => print(gpuGet(value, 0) ?: 0.0)  // 19.0\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuLoad",
        "Reads count little-endian IEEE-754 binary32 values starting at a byte offset of a file into a `GpuBuffer<float>`, widening each exactly. A missing file or a short read is an Error carrying the operating system's reason.",
        ["path" => "The file to read", "offset" => "The byte offset the first value starts at", "count" => "How many values to read"],
        "// Supply a file containing little-endian float32 values.\nmatch gpuLoad(\"weights.bin\", 0, 3) {\n    Success { value } => print(\"${gpuLength(value)} values\")\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuLoadTensor",
        "Reads one named tensor out of a safetensors file into a `GpuBuffer<float>`, widening F64, F32, F16 or BF16 storage exactly. An unknown name, another dtype or a truncated file returns Error.",
        ["path" => "The safetensors file", "name" => "The tensor's name in the header"],
        "// Supply a safetensors file containing the named tensor.\nmatch gpuLoadTensor(\"model.safetensors\", \"wte.weight\") {\n    Success { value } => print(\"${gpuLength(value)} values\")\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuDevice",
        "Returns the name of the backend GPU kernels and products run on: \"host\", or the adapter OSPREY_GPU selected, such as \"metal:0 (Apple M4 Max)\" or \"opencl:0 (Apple M4 Max)\".",
        [],
        "print(gpuDevice())  // host",
    ),
];
