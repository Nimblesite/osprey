//! The module KLEE explores [SYMEX-CODEGEN] [SYMEX-RUNTIME-BITCODE]
//! (docs/specs/compiler/KleeCodegen.md). The program is lowered once with
//! debug locations and the abort-on-guard switch, then given the target the
//! runtime bitcode was built for and the stream cap the mock libc reads.

use crate::project::CompilationInput;

/// KLEE lowers `va_start` for the x86-64 ABI, so every module it loads is
/// x86-64 Linux whatever the host; the runtime bitcode is built the same way.
const TARGET_HEADER: &str = "target datalayout = \"e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n\
target triple = \"x86_64-unknown-linux-gnu\"\n";

pub(crate) fn symbolic_module(
    input: &CompilationInput,
    input_bytes: u64,
) -> Result<String, String> {
    let source = osprey_codegen::DebugSource::from_path(input.debug_path());
    osprey_codegen::compile_program_symbolic(input.backend_program(), source)
        .map(|ir| wrap(&ir, input_bytes))
        .map_err(|error| error.to_string())
}

/// Prefix the target, append the strong `osp_hunt_input_bytes` that overrides
/// the mock's weak default [SYMEX-MOCK-STREAM].
pub(crate) fn wrap(ir: &str, input_bytes: u64) -> String {
    format!("{TARGET_HEADER}{ir}\n@osp_hunt_input_bytes = global i64 {input_bytes}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_module_targets_x86_64_and_sets_the_stream_cap() {
        let module = wrap("define i32 @main() {\n  ret i32 0\n}", 8);
        assert!(module.starts_with("target datalayout"));
        crate::testkit::shows(&module, &["target triple = \"x86_64-unknown-linux-gnu\""]);
        assert!(module.ends_with("@osp_hunt_input_bytes = global i64 8\n"));
    }

    #[test]
    fn program_module_carries_locations_for_findings() {
        let source = "print(\"hello\")\n";
        let parsed = osprey_syntax::parse_program(source);
        let input = CompilationInput::script("hunt.osp", source.to_string(), parsed.program);
        let module = symbolic_module(&input, 16).expect("symbolic module");
        assert!(module.contains("!DILocation") && module.contains("hunt.osp"));
        crate::testkit::shows(&module, &["@osp_hunt_input_bytes = global i64 16"]);
    }
}
