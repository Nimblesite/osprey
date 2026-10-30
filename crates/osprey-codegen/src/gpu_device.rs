//! Device emission for extracted GPU kernels — implements [GPU-BACKEND-DEVICE]
//! (docs/specs/runtime/0034-GPUComputation.md).
//!
//! Every kernel [`crate::gpu_kernel`] lifts is also compiled to the shared
//! device dialect (`compiler/runtime/gpu_f64.metal` and its two companions),
//! which Metal and `OpenCL` compile at run time together with the binary64
//! library. The kernel's body is emitted once from its AST ([`emit`]); the
//! entry point wrapping it depends on the combinator — one thread per element
//! for `gpuMap`/`gpuZipWith`, one thread looping in order for `gpuFold` — and
//! is written when the launch site is lowered ([`launch`]). The program's
//! kernels travel as one string constant every launch site hands the runtime,
//! so no symbol has to exist in a program that never reaches a device.
//!
//! A kernel the dialect cannot express keeps its host lowering and its launch
//! site declines by name: the runtime runs the host loop when the host is
//! selected and aborts when a device is ([GPU-BACKEND-DEVICE]).
//! Foreign calls are rejected during code generation before a launch is emitted.

mod dialect;
mod emit;
mod launch;
mod loops;

pub(crate) use launch::{declined, launch_fold, launch_map};

use crate::builder::Codegen;
use crate::llty::LType;
use osprey_ast::{Expr, Parameter, Program, Stmt};
use std::collections::HashMap;
use std::fmt::Write as _;

/// The device dialect's spelling of a kernel's scalar word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    Int,
    Float,
    Bool,
}

impl Ty {
    fn of(ty: LType) -> Option<Ty> {
        match ty {
            LType::I64 => Some(Ty::Int),
            LType::Double => Some(Ty::Float),
            LType::I1 => Some(Ty::Bool),
            _ => None,
        }
    }

    fn spelling(self) -> &'static str {
        match self {
            Ty::Int => "long",
            Ty::Float => "ulong",
            Ty::Bool => "bool",
        }
    }

    /// A raw buffer word as a value of this type.
    fn value_of(self, word: &str) -> String {
        match self {
            Ty::Int => format!("(long)({word})"),
            Ty::Float => word.to_string(),
            Ty::Bool => format!("({word} != 0ul)"),
        }
    }

    /// A value of this type as the raw buffer word that stores it.
    fn word_of(self, value: &str) -> String {
        match self {
            Ty::Float => value.to_string(),
            Ty::Int | Ty::Bool => format!("(ulong)({value})"),
        }
    }
}

/// One uniform of a device kernel: a scalar word or a buffer handle, in the
/// order the launch site passes them ([GPU-KERNEL-EXTRACT] uniforms).
#[derive(Clone, Debug)]
pub(crate) enum Uniform {
    Scalar(Ty),
    Buffer,
}

impl Uniform {
    fn kind(&self) -> char {
        match self {
            Uniform::Scalar(_) => 's',
            Uniform::Buffer => 'b',
        }
    }
}

/// A kernel body compiled to the device dialect, shape-independent: `lines`
/// compute `result` from the slot values `p0`, `p1`, … and the uniforms
/// `u0`, `u1`, … that the entry point binds.
#[derive(Clone, Debug)]
pub(crate) struct DeviceBody {
    lines: Vec<String>,
    result: String,
    result_ty: Ty,
    params: Vec<Ty>,
    uniforms: Vec<Uniform>,
}

#[derive(Clone, Debug)]
pub(crate) enum DeviceFailure {
    Unsupported(String),
    Foreign(String),
}

impl From<String> for DeviceFailure {
    fn from(reason: String) -> Self {
        Self::Unsupported(reason)
    }
}

impl From<&str> for DeviceFailure {
    fn from(reason: &str) -> Self {
        Self::Unsupported(reason.to_owned())
    }
}

impl DeviceFailure {
    pub(super) fn decline_reason(self) -> crate::error::Result<String> {
        match self {
            Self::Unsupported(reason) => Ok(reason),
            Self::Foreign(name) => Err(crate::error::CodegenError::unsupported(format!(
                "foreign function `{name}` has no device form"
            ))),
        }
    }
}

/// An installed entry point: its name and its uniforms' kinds, which the
/// launch site passes to the runtime.
#[derive(Clone, Debug)]
pub(crate) struct DeviceKernel {
    name: String,
    kinds: String,
}

/// Module-wide device emission state.
#[derive(Default)]
pub(crate) struct DeviceState {
    /// Every entry point installed so far, in launch-site order.
    source: String,
    installed: usize,
    /// Every function definition, for inlining a kernel's helpers.
    defs: HashMap<String, (Vec<Parameter>, Expr)>,
    /// File-scope immutable bindings with literal values.
    constants: HashMap<String, Expr>,
    /// Bindings that merely rename another name (`let f = addF`, or the
    /// `let f$stage = g` a static handler arm binds), resolved to the name
    /// at the end of the chain, so a kernel passed through such a binding is
    /// still found by what it names.
    pub(crate) aliases: HashMap<String, String>,
}

/// Prefix of every installed entry point; the suffix is the install order, a
/// pure function of AST walk order ([FLAVOR-IR-EQUIV]).
const KERNEL_PREFIX: &str = "osprey_kernel_";
/// The module global holding the program's kernels, and the pointer to it the
/// launch sites load.
const SOURCE_GLOBAL: &str = "@osprey.gpu.kernels";
const SOURCE_POINTER: &str = "@osprey.gpu.kernels.ptr";

/// Record the definitions and constants a kernel body may reach.
pub(crate) fn record(cg: &mut Codegen, program: &Program) {
    for statement in &program.statements {
        match statement {
            Stmt::Function {
                name,
                parameters,
                body,
                ..
            } => {
                let _ = cg
                    .device
                    .defs
                    .insert(name.clone(), (parameters.clone(), body.clone()));
            }
            Stmt::Let {
                name,
                mutable: false,
                value,
                ..
            } if dialect::is_literal(value) => {
                let _ = cg.device.constants.insert(name.clone(), value.clone());
            }
            _ => {}
        }
    }
}

/// Record whether `let name = value` merely renames another name; any other
/// value forgets an alias the name held before.
pub(crate) fn note_alias(cg: &mut Codegen, name: &str, value: &Expr) {
    let _ = cg.device.aliases.remove(name);
    if let Expr::Identifier(target) = dialect::callee(value) {
        let target = aliased(cg, target);
        let _ = cg.device.aliases.insert(name.to_string(), target);
    }
}

/// The name at the end of `name`'s alias chain (`name` itself when it is none).
pub(crate) fn aliased(cg: &Codegen, name: &str) -> String {
    cg.device
        .aliases
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

/// Compile a kernel's body for the device, or the reason it cannot run there.
/// `slots` are the kernel's own parameter types in call order and `captures`
/// its free variables in launch order.
pub(crate) fn compile(
    cg: &Codegen,
    parameters: &[Parameter],
    body: &Expr,
    slots: &[LType],
    captures: &[crate::closure::Capture],
) -> Result<DeviceBody, DeviceFailure> {
    let params = slots
        .iter()
        .map(|slot| Ty::of(*slot).ok_or_else(|| format!("a {slot} slot is not a device word")))
        .collect::<Result<Vec<_>, _>>()?;
    let uniforms = captures
        .iter()
        .map(|capture| uniform_of(&capture.val).map(|uniform| (capture.name.clone(), uniform)))
        .collect::<Result<Vec<_>, _>>()?;
    emit::body(cg, parameters, body, &params, &uniforms)
}

fn uniform_of(value: &crate::llty::Value) -> Result<Uniform, String> {
    if value.result_inner.is_some() {
        return Err("a Result capture has no device representation".into());
    }
    match Ty::of(value.ty) {
        Some(ty) => Ok(Uniform::Scalar(ty)),
        None if value.ty == LType::Ptr
            && value
                .osp_ty
                .as_deref()
                .is_some_and(crate::gpu::is_buffer_owner) =>
        {
            Ok(Uniform::Buffer)
        }
        None => Err(format!(
            "a {} capture has no device representation",
            value.osp_ty.as_deref().unwrap_or("handle")
        )),
    }
}

/// The two entry-point shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// One thread per element over `inputs` buffers.
    Map { inputs: usize },
    /// One thread folding `in0` into the accumulator seeded in `out[0]`.
    Fold,
}

/// Install an entry point around `body` and hand back its name and kinds.
fn install(cg: &mut Codegen, body: &DeviceBody, shape: Shape) -> DeviceKernel {
    let name = format!("{KERNEL_PREFIX}{}", cg.device.installed);
    cg.device.installed += 1;
    cg.device.source.push_str(&entry(&name, body, shape));
    DeviceKernel {
        name,
        kinds: body.uniforms.iter().map(Uniform::kind).collect(),
    }
}

/// The fixed leading parameters every entry point takes, which the runtime
/// binds first: the element buffers, the result, the fault word, the count.
const FIXED_PARAMETERS: &str = "GPU_GLOBAL const ulong *in0 GPU_BUFFER(0), GPU_GLOBAL const ulong *in1 GPU_BUFFER(1), \
GPU_GLOBAL ulong *out GPU_BUFFER(2), GPU_GLOBAL ulong *fault GPU_BUFFER(3), GPU_PARAM count GPU_BUFFER(4)";

fn entry(name: &str, body: &DeviceBody, shape: Shape) -> String {
    let mut text = format!(
        "kernel void {name}({FIXED_PARAMETERS}{} GPU_THREAD) {{\n",
        uniform_parameters(body)
    );
    text.push_str(&uniform_bindings(body));
    match shape {
        Shape::Map { inputs } => map_entry(&mut text, body, inputs),
        Shape::Fold => fold_entry(&mut text, body),
    }
    text.push_str("}\n");
    text
}

fn uniform_parameters(body: &DeviceBody) -> String {
    let mut index = 5;
    let mut text = String::new();
    for (i, uniform) in body.uniforms.iter().enumerate() {
        match uniform {
            Uniform::Scalar(_) => {
                let _ = write!(text, ", GPU_PARAM u{i} GPU_BUFFER({index})");
                index += 1;
            }
            Uniform::Buffer => {
                let _ = write!(
                    text,
                    ", GPU_GLOBAL const ulong *u{i} GPU_BUFFER({index}), GPU_PARAM u{i}_len GPU_BUFFER({})",
                    index + 1
                );
                index += 2;
            }
        }
    }
    text
}

/// Scalar uniforms arrive as words; bind each at its type under the name the
/// body reads (`u{i}_v`).
fn uniform_bindings(body: &DeviceBody) -> String {
    body.uniforms
        .iter()
        .enumerate()
        .filter_map(|(i, uniform)| match uniform {
            Uniform::Scalar(ty) => Some(format!(
                "  {} u{i}_v = {};\n",
                ty.spelling(),
                ty.value_of(&format!("u{i}"))
            )),
            Uniform::Buffer => None,
        })
        .collect()
}

fn map_entry(text: &mut String, body: &DeviceBody, inputs: usize) {
    text.push_str("  ulong gid = GPU_INDEX;\n  if (gid >= count) return;\n");
    for (i, ty) in body.params.iter().enumerate().take(inputs) {
        let _ = writeln!(
            text,
            "  {} p{i} = {};",
            ty.spelling(),
            ty.value_of(&format!("in{i}[gid]"))
        );
    }
    indented(text, &body.lines, "  ");
    let _ = writeln!(
        text,
        "  out[gid] = {};",
        body.result_ty.word_of(&body.result)
    );
}

fn fold_entry(text: &mut String, body: &DeviceBody) {
    let acc = body.params.first().copied().unwrap_or(Ty::Int);
    let elem = body.params.get(1).copied().unwrap_or(Ty::Int);
    let _ = writeln!(
        text,
        "  if (GPU_INDEX != 0) return;\n  {} acc = {};\n  for (ulong i = 0; i < count; i++) {{",
        acc.spelling(),
        acc.value_of("out[0]")
    );
    let _ = writeln!(
        text,
        "    {} p0 = acc;\n    {} p1 = {};",
        acc.spelling(),
        elem.spelling(),
        elem.value_of("in0[i]")
    );
    indented(text, &body.lines, "    ");
    let _ = writeln!(
        text,
        "    acc = {};\n  }}\n  out[0] = {};",
        body.result,
        acc.word_of("acc")
    );
}

fn indented(text: &mut String, lines: &[String], indent: &str) {
    for line in lines {
        let _ = writeln!(text, "{indent}{line}");
    }
}

/// Emit the module constant holding every installed kernel, once lowering is
/// complete. A module with no device kernel emits nothing.
pub(crate) fn publish(cg: &mut Codegen) {
    if cg.device.source.is_empty() {
        return;
    }
    let (escaped, len) = crate::builder::escape_c_string(&cg.device.source);
    cg.add_global(format!(
        "{SOURCE_GLOBAL} = private unnamed_addr constant [{len} x i8] c\"{escaped}\""
    ));
    cg.add_global(format!(
        "{SOURCE_POINTER} = internal constant i8* getelementptr inbounds ([{len} x i8], [{len} x i8]* {SOURCE_GLOBAL}, i64 0, i64 0)"
    ));
}
