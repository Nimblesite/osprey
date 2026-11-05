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
        "Returns a dense GpuBuffer in Success, or Error with the allocation reason, for a list of numeric scalars at any width or bools. Also consumes an iterator pipeline, replaying its map/filter stages into the buffer with no list in between.",
        ["source" => "The scalar list or iterator to copy into a buffer"],
        "match toGpu([1, 2, 3, 4]) {\n    Success { value } => { let buffer = value\n    print(gpuLength(buffer))\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "fromGpu",
        "Returns a host list in Success, keeping its element type; a failed device download returns Error with its reason. A valid empty buffer succeeds with an empty list.",
        ["buffer" => "The buffer to copy back to a list"],
        "match toGpu([1, 2]) {\n    Success { value } => { let buffer = value\n    match fromGpu(buffer) {\n        Success { value } => forEachList(value, print)\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuLength",
        "Returns a GpuBuffer's element count. O(1).",
        ["buffer" => "The buffer to measure"],
        "match toGpu([1, 2, 3]) {\n    Success { value } => { let buffer = value\n    print(gpuLength(buffer))\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuMap",
        "Applies a pure kernel to every buffer element independently, one thread per element when OSPREY_GPU selects Metal or OpenCL. The compiler rejects a kernel that performs any effect a handler must discharge at runtime; an effect a static handler has already erased is gone before the check. Allocation, dispatch or synchronization failure returns Error with its reason.",
        ["buffer" => "The source buffer", "kernel" => "The pure per-element function"],
        "match toGpu([1, 2, 3]) {\n    Success { value } => { let buffer = value\n    match gpuMap(buffer, fn(x) => wrapMul(x, x)) {\n        Success { value } => match gpuGet(value, 2) {\n            Success { value } => print(value)\n            Error { message } => print(message)\n        }\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuFold",
        "Reduces a buffer to one scalar with a pure combine function, applied left to right on every backend. A device runs the fold on one thread, so fold a large buffer by rows and then fold the rows. Allocation, dispatch or synchronization failure returns Error with its reason.",
        ["buffer" => "The buffer to reduce", "initial" => "The initial scalar accumulator", "combine" => "The pure (accumulator, element) function"],
        "match toGpu([1, 2, 3]) {\n    Success { value } => { let buffer = value\n    match gpuFold(buffer, 0, wrapAdd) {\n        Success { value } => print(value)\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuZipWith",
        "Combines two buffers elementwise with a pure binary kernel. The result takes the shorter operand's length. Allocation, dispatch or synchronization failure returns Error with its reason.",
        ["a" => "The left buffer", "b" => "The right buffer", "kernel" => "The pure (a, b) element function"],
        "match toGpu([2, 3]) {\n    Success { value } => { let a = value\n    match toGpu([4, 5]) {\n        Success { value } => { let b = value\n        match gpuZipWith(a, b, wrapMul) {\n            Success { value } => match gpuGet(value, 1) {\n                Success { value } => print(value)\n                Error { message } => print(message)\n            }\n            Error { message } => print(message)\n        }\n        }\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuIota",
        "Returns the index buffer [0, n), or Error for a negative count, byte-size overflow or allocation failure. Zero succeeds with an empty buffer.",
        ["n" => "The element count"],
        "match gpuIota(4) {\n    Success { value } => match fromGpu(value) {\n        Success { value } => forEachList(value, print)\n        Error { message } => print(message)\n    }\n    Error { message } => print(message)\n}  // 0, 1, 2, 3",
    ),
    builtin_doc!(
        "gpuGet",
        "Bounds-checked read of one element at the buffer's element type. Out of bounds returns Error.",
        ["buffer" => "The buffer to read", "index" => "The element index"],
        "match toGpu([10, 20]) {\n    Success { value } => { let buffer = value\n    match gpuGet(buffer, 1) {\n        Success { value } => print(value)\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuScan",
        "Inclusive prefix scan: element i is combine folded left to right through element i. Allocation, dispatch or synchronization failure returns Error with its reason.",
        ["buffer" => "The buffer to scan", "initial" => "The initial scalar accumulator", "combine" => "The pure (accumulator, element) function"],
        "match toGpu([1, 2, 3]) {\n    Success { value } => { let buffer = value\n    match gpuScan(buffer, 0, wrapAdd) {\n        Success { value } => match fromGpu(value) {\n            Success { value } => forEachList(value, print)\n            Error { message } => print(message)\n        }\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuFilter",
        "Stream compaction: keeps the elements a pure predicate accepts, preserving order. Allocation, dispatch or synchronization failure returns Error with its reason.",
        ["buffer" => "The source buffer", "predicate" => "The pure element predicate"],
        "match toGpu([1, 2, 3, 4]) {\n    Success { value } => { let buffer = value\n    match gpuFilter(buffer, fn(x) => x % 2 == 0) {\n        Success { value } => match fromGpu(value) {\n            Success { value } => forEachList(value, print)\n            Error { message } => print(message)\n        }\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuMatMul",
        "The dense row-major product of a rows x inner float buffer and an inner x cols one, as a rows x cols buffer. Both operands have the same float or float32 element type. Every element accumulates in k order from 0.0 at that width, without fused multiply-add; a length that disagrees with the stated shape is an Error naming the operand.",
        ["left" => "The rows x inner buffer", "right" => "The inner x cols buffer", "rows" => "The left operand's row count", "inner" => "The shared dimension", "cols" => "The right operand's column count"],
        "match toGpu([1.0, 2.0, 3.0, 4.0]) {\n    Success { value } => { let left = value\n    match toGpu([5.0, 6.0, 7.0, 8.0]) {\n        Success { value } => { let right = value\n        match gpuMatMul(left, right, 2, 2, 2) {\n            Success { value } => match gpuGet(value, 0) {\n                Success { value } => print(value)\n                Error { message } => print(message)\n            }\n            Error { message } => print(message)\n        }\n        }\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuMatMulQuant",
        "The product of a rows x inner float32 matrix and cols rows of quantized weights in the named GGUF block format (Q4_K: 256 weights per 144-byte block), as a rows x cols float32 buffer. Each weight is dequantized, multiplied and added in ggml's block order with every step rounded, in four block-range parts added in order when rows x cols is at most 2^20 and one part otherwise, identically on the host and every device. A length, element type or inner dimension the shape disagrees with returns Error.",
        ["x" => "The rows x inner float32 buffer", "weights" => "The uint8 buffer holding cols rows of blocks, as the GGUF tensor stores them", "type" => "The GGUF tensor type name, Q4_K", "rows" => "The row count of x", "inner" => "The shared dimension, a multiple of 256", "cols" => "The weight row count"],
        "match gpuMapFile(\"model.gguf\", 10996640, 14745600) {\n    Success { value } => { let weights = value\n    match toGpu(xs) {\n        Success { value } => match gpuMatMulQuant(value, weights, \"Q4_K\", 1, 5120, 5120) {\n            Success { value } => print(gpuLength(value))\n            Error { message } => print(message)\n        }\n        Error { message } => print(message)\n    }\n    }\n    Error { message } => print(message)\n}"
    ),
    builtin_doc!(
        "gpuLoad",
        "Reads count little-endian IEEE-754 binary32 values starting at a byte offset of a file into a `GpuBuffer<float32>`, preserving the binary32 words. A missing file or a short read is an Error carrying the operating system's reason.",
        ["path" => "The file to read", "offset" => "The byte offset the first value starts at", "count" => "How many values to read"],
        "// Supply a file containing little-endian float32 values.\nmatch gpuLoad(\"weights.bin\", 0, 3) {\n    Success { value } => print(\"${gpuLength(value)} values\")\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuLoadTensor",
        "Reads one named tensor out of a safetensors file into the requested float buffer type: F64 requires float, F32 and BF16 require float32, and F16 requires float16. An unknown name, mismatched request, unsupported dtype or truncated file returns Error.",
        ["path" => "The safetensors file", "name" => "The tensor's name in the header"],
        "let loaded: Result<GpuBuffer<float32>, Error> = gpuLoadTensor(\"model.safetensors\", \"wte.weight\")\nmatch loaded {\n    Success { value } => print(\"${gpuLength(value)} values\")\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "gpuMapFile",
        "Maps count elements of a file, starting at a byte offset, as a read-only `GpuBuffer` of the requested numeric type. Nothing is copied: on unified memory the GPU reads the file's own pages.",
        ["path" => "The file to map", "offset" => "The byte offset, a multiple of the element width", "count" => "How many elements to map"],
        "let weights: Result<GpuBuffer<float32>, Error> = gpuMapFile(\"weights.bin\", 0, 1024)\nprint(\"mapped\")"
    ),
    builtin_doc!(
        "gpuDevice",
        "Returns the name of the backend GPU kernels and products run on: \"host\", or the adapter OSPREY_GPU selected, such as \"metal:0 (Apple M4 Max)\" or \"opencl:0 (Apple M4 Max)\".",
        [],
        "print(gpuDevice())  // host",
    ),
];
