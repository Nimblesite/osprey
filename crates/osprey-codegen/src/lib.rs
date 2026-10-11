//! LLVM IR (text) code generation for Osprey.
//!
//! The backend walks the AST and prints LLVM assembly that clang compiles and
//! links against libc and the prebuilt C runtime archives in `compiler/bin/`
//! (`libfiber_runtime.a` / `libhttp_runtime.a`). Two anchors define correct
//! output: the C runtime ABI (those archives' symbols and conventions) and the
//! `.expectedoutput` goldens beside every program under `tests/`, compared
//! byte-for-byte end-to-end by `crates/run_test_corpus.sh` — under each memory
//! backend, and again on wasm32 with `OSPREY_TARGET=wasm32`.
//! Constructs the backend does not lower return
//! [`CodegenError::Unsupported`] — it never emits a placeholder.
//!
//! Public surface: [`compile_program`] turns a parsed [`osprey_ast::Program`]
//! into a module string.

mod aggregate;
mod anybox;
mod arc;
mod arithmetic;
mod builder;
mod builtin_values;
mod call;
mod cast;
mod closure;
mod collections;
mod conv;
mod coverage;
mod curry;
mod effect_generics;
mod effect_mailbox;
mod effects;
mod error;
mod expr;
mod extern_call;
mod fiber;
#[cfg(test)]
mod freevars_tests;
mod genfn;
mod globals;
mod gpu;
mod gpu_kernel;
mod iter;
mod listlit;
mod llty;
mod loops;
mod lower;
mod meta;
mod monofn;
mod pattern;
mod result;
mod runtime;
mod stmt;
mod strings;
mod testing;
#[cfg(test)]
#[path = "../../testkit.rs"]
mod testkit;
mod types;

pub use error::{CodegenError, Result};
pub use gpu_kernel::{GpuKernelMode, GPU_KERNELS_ENV};
pub use llty::{LType, Value};
pub use lower::{
    compile_library, compile_program, compile_program_coverage, compile_program_debug,
};
pub use osprey_debug::DebugSource;

/// Every identifier referenced anywhere in `program` — function bodies, lets,
/// nested modules. The CLI's capability sandbox uses this to detect gated
/// builtins (`httpGet`, `readFile`, …) without compiling.
#[must_use]
pub fn referenced_idents(program: &osprey_ast::Program) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for s in &program.statements {
        stmt_idents(s, &mut out);
    }
    out
}

fn stmt_idents(s: &osprey_ast::Stmt, out: &mut std::collections::BTreeSet<String>) {
    use osprey_ast::Stmt;
    match s {
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } => {
            osprey_ast::freevars::free_idents(value, out);
        }
        Stmt::Expr { value: e, .. } | Stmt::Function { body: e, .. } => {
            osprey_ast::freevars::free_idents(e, out);
        }
        Stmt::Module { body, .. } => {
            for item in body {
                stmt_idents(&item.declaration, out);
            }
        }
        Stmt::Namespace { body, .. } => {
            for inner in body {
                stmt_idents(inner, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "ir_tests.rs"]
mod tests;
