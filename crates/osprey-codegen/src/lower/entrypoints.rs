use super::{compile_module, compile_program_with_options};
use crate::builder::{CodegenOptions, ImportAbi};
use crate::error::Result;
use osprey_ast::Program;
use osprey_debug::DebugSource;

/// Compile a whole program to an LLVM IR module (text), driven by the inferred
/// types in [`osprey_types::ProgramTypes`].
///
/// # Errors
///
/// Returns `Err` if any function body, top-level statement, or `main`
/// expression contains a construct that cannot be lowered to LLVM IR.
pub fn compile_program(program: &Program) -> Result<String> {
    compile_program_with_options(program, CodegenOptions::default())
}

/// Compile app logic whose module state remains live after initialization.
/// The host owns process lifetime and calls exported functions after entry
/// returns. [IOS-TARGET-ENTRY] [WASM-WEB-ABI]
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_library(program: &Program) -> Result<String> {
    compile_module(program, host_options(), true)
}

/// Compile a whole program for wasm32, whose 4-byte pointers change the
/// allocation-mask stride ([`crate::meta`]).
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_wasm_program(program: &Program) -> Result<String> {
    compile_program_with_options(program, wasm_options())
}

/// Compile app logic for wasm32 whose module state remains live after the
/// entry returns ([WASM-WEB-ABI]), with wasm32 pointer-width metadata.
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_wasm_library(program: &Program) -> Result<String> {
    compile_module(program, wasm_options(), true)
}

/// The codegen options every wasm32 compilation shares.
fn wasm_options() -> CodegenOptions {
    CodegenOptions {
        wasm32: true,
        ..CodegenOptions::default()
    }
}

/// Compile a whole program with LLVM/DWARF debug metadata rooted at `source`.
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_program_debug(program: &Program, source: DebugSource) -> Result<String> {
    compile_program_with_options(
        program,
        CodegenOptions {
            debug_source: Some(source),
            ..CodegenOptions::default()
        },
    )
}

/// Compile a whole program for symbolic execution [SYMEX-CODEGEN]: debug
/// metadata rooted at `source` so the engine names Osprey lines, and fatal
/// paths that `abort` so it reports them.
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_program_symbolic(program: &Program, source: DebugSource) -> Result<String> {
    compile_program_with_options(
        program,
        CodegenOptions {
            debug_source: Some(source),
            symbolic: true,
            ..CodegenOptions::default()
        },
    )
}

/// Compile a whole program with line-coverage instrumentation
/// [TESTING-COVERAGE-CODEGEN].
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_program_coverage(program: &Program) -> Result<String> {
    compile_program_with_options(
        program,
        CodegenOptions {
            coverage: true,
            ..CodegenOptions::default()
        },
    )
}

/// Compile an entry-only program whose C imports are converted by its host adapter.
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_host_program(program: &Program) -> Result<String> {
    compile_program_with_options(program, host_options())
}

fn host_options() -> CodegenOptions {
    CodegenOptions {
        import_abi: ImportAbi::HostAdapted,
        ..CodegenOptions::default()
    }
}
