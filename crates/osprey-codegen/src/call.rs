//! The single "declare + call" emission seam. Every runtime / libc builtin in
//! `collections.rs`, `strings.rs`, `iter.rs`, … goes through here instead of
//! repeating the `add_extern(declare …)` + `fresh_reg` + `emit("… = call …")`
//! triple by hand. `params` is the LLVM parameter-type list (`"i8*, i64"`);
//! `args` the rendered operands, one per parameter in the same order.

use crate::builder::Codegen;

/// Render `declare {ret} @{cname}({params})` once (idempotent) and the typed
/// argument list `{ty} {op}, …` shared by [`Codegen::call`] / [`Codegen::call_void`].
fn declare_and_args(
    cg: &mut Codegen,
    ret: &str,
    cname: &str,
    params: &str,
    args: &[&str],
) -> String {
    cg.add_extern(format!("declare {ret} @{cname}({params})"));
    params
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .zip(args.iter())
        .map(|(ty, op)| format!("{ty} {op}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Codegen {
    /// Declare `cname` and emit `r = call {ret} @{cname}(args)`, returning the
    /// result register `r`.
    pub(crate) fn call(&mut self, ret: &str, cname: &str, params: &str, args: &[&str]) -> String {
        let typed = declare_and_args(self, ret, cname, params, args);
        self.emit_call_result(ret, cname, &typed)
    }

    /// Declare `cname` and emit a `void` call (no result register).
    pub(crate) fn call_void(&mut self, cname: &str, params: &str, args: &[&str]) {
        let typed = declare_and_args(self, "void", cname, params, args);
        let _ = self.emit_call_result("void", cname, &typed);
    }

    /// C `void` has no SSA result; its Osprey Unit value is the scalar zero.
    pub(crate) fn emit_call_result(&mut self, ret: &str, name: &str, typed: &str) -> String {
        let instruction = format!("call {ret} @{name}({typed})");
        if ret == "void" {
            self.emit(instruction);
            String::from("0")
        } else {
            self.emit_reg(instruction)
        }
    }
}
