use super::Linkage;
use osprey_debug::DebugSource;

/// The layer that converts an Osprey import to its C ABI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ImportAbi {
    #[default]
    Direct,
    HostAdapted,
}

/// Code generation switches that alter the emitted module without changing
/// Osprey semantics.
#[derive(Debug, Clone, Default)]
pub(crate) struct CodegenOptions {
    /// Source file identity used for LLVM/DWARF debug metadata.
    pub debug_source: Option<DebugSource>,
    /// Instrument coverable lines with hit counters [TESTING-COVERAGE-CODEGEN].
    pub coverage: bool,
    /// Which lowering the GPU combinators use for their kernels
    /// [GPU-KERNEL-EXTRACT].
    pub gpu_kernels: crate::gpu_kernel::GpuKernelMode,
    /// Which functions the module exposes to the linker [CODEGEN-LINKAGE].
    pub linkage: Linkage,
    /// Emit for symbolic execution [SYMEX-CODEGEN]: fatal paths `abort`
    /// instead of `exit`, so the engine reports them as errors.
    pub symbolic: bool,
    /// Lower for wasm32, whose pointers are 4 bytes wide: allocation metadata
    /// must then mask pointer-stride words, not native 8-byte words.
    pub wasm32: bool,
    /// Preserve the internal import ABI for the native host adapter.
    pub import_abi: ImportAbi,
}
