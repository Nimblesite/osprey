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
mod entries;
mod launch;
mod loops;
mod numeric;

pub(crate) use launch::{launch_fold, launch_map, launch_sequence};

use crate::builder::Codegen;
use crate::llty::LType;
use osprey_ast::{Expr, Parameter, Program, Stmt};
use osprey_types::numeric::Numeric;
use std::collections::HashMap;

/// The device dialect's spelling of a kernel's scalar word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    Num(Numeric),
    Bool,
}

impl Ty {
    pub(crate) const INT: Ty = Ty::Num(Numeric::INT);
    pub(crate) const FLOAT: Ty = Ty::Num(Numeric::FLOAT);
    pub(super) const FLOAT32: Ty = Ty::Num(Numeric::FLOAT32);

    fn of(ty: LType) -> Option<Ty> {
        match ty {
            LType::Num(n) => Some(Ty::Num(n)),
            LType::I1 => Some(Ty::Bool),
            _ => None,
        }
    }

    /// The type arithmetic runs at: `float16` computes in binary32 and narrows
    /// once at the store boundary ([GPU-DEVICE-NATIVE]).
    fn compute(self) -> Ty {
        match self {
            Ty::Num(n) if n.is_float() && n.bits == 16 => Ty::FLOAT32,
            other => other,
        }
    }

    /// The source spelling, for diagnostics.
    fn name(self) -> &'static str {
        match self {
            Ty::Num(n) => n.name(),
            Ty::Bool => "bool",
        }
    }

    /// The storage spelling: a buffer element's type (`half` for `float16`).
    fn spelling(self) -> &'static str {
        match self {
            Ty::Num(n) => n.device(),
            Ty::Bool => "bool",
        }
    }

    /// The compute spelling: the variable type inside a kernel (`float` for
    /// `float16`).
    fn compute_spelling(self) -> &'static str {
        self.compute().spelling()
    }

    /// A raw buffer word as a value of this type: `float16` storage reads
    /// through binary32, sized integers cast from the word.
    fn value_of(self, word: &str) -> String {
        match self {
            Ty::Num(n) if n.is_float() && n.bits == 16 => format!("((float)({word}))"),
            Ty::Num(n) if n.is_float() => word.to_string(),
            Ty::Num(n) => format!("({})({word})", n.device()),
            Ty::Bool => format!("({word} != 0ul)"),
        }
    }

    /// A value of this type as the raw buffer word that stores it: a `float16`
    /// result narrows to `half` in one rounding.
    fn word_of(self, value: &str) -> String {
        match self {
            Ty::Num(n) if n.is_float() && n.bits == 16 => format!("(half)({value})"),
            Ty::Num(n) if n.is_float() => value.to_string(),
            Ty::Num(_) | Ty::Bool => format!("(ulong)({value})"),
        }
    }
}

/// One uniform of a device kernel: a scalar word or a buffer handle, in the
/// order the launch site passes them ([GPU-KERNEL-EXTRACT] uniforms).
#[derive(Clone, Debug)]
pub(crate) enum Uniform {
    Scalar(Ty),
    Buffer(Ty),
}

impl Uniform {
    fn kind(&self) -> char {
        match self {
            Uniform::Scalar(_) => 's',
            Uniform::Buffer(_) => 'b',
        }
    }
}

/// A kernel body compiled to the device dialect, shape-independent: `lines`
/// compute `result` from the slot values `p0`, `p1`, … and the uniforms
/// `u0`, `u1`, … that the entry point binds. `params` and `result_ty` are the
/// compute types (`float16` is binary32 here); `param_storage` and
/// `result_storage` are the buffer element types (`float16` is `half`).
#[derive(Clone, Debug)]
pub(crate) struct DeviceBody {
    lines: Vec<String>,
    result: String,
    result_storage: Ty,
    params: Vec<Ty>,
    param_storage: Vec<Ty>,
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
    constants: HashMap<String, (Option<Ty>, Expr)>,
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
                position,
                ..
            } if dialect::is_literal(value) => {
                let ty = cg
                    .prog
                    .let_type(None, *position)
                    .and_then(Numeric::of_type)
                    .map(Ty::Num);
                let _ = cg
                    .device
                    .constants
                    .insert(name.clone(), (ty, value.clone()));
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
    result: LType,
) -> Result<DeviceBody, DeviceFailure> {
    let storages = slots
        .iter()
        .map(|slot| {
            Ty::of(*slot).ok_or_else(|| format!("a {} slot is not a device word", slot_name(*slot)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let params: Vec<Ty> = storages.iter().map(|ty| ty.compute()).collect();
    let result_storage = Ty::of(result).unwrap_or(Ty::INT);
    let uniforms = captures
        .iter()
        .map(|capture| uniform_of(&capture.val).map(|uniform| (capture.name.clone(), uniform)))
        .collect::<Result<Vec<_>, _>>()?;
    emit::body(
        cg,
        parameters,
        body,
        &params,
        &storages,
        &uniforms,
        result_storage,
    )
}

/// The source name of a slot for a decline reason, not its LLVM spelling.
fn slot_name(ty: LType) -> &'static str {
    match ty {
        LType::Num(n) => n.name(),
        LType::I1 => "bool",
        LType::Str | LType::Ptr | LType::Any => "a handle",
    }
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
            crate::gpu::tagged_elem(value)
                .and_then(Ty::of)
                .map(Uniform::Buffer)
                .ok_or_else(|| "a captured buffer has no concrete element type".to_string())
        }
        None => Err(format!(
            "a {} capture has no device representation",
            value.osp_ty.as_deref().unwrap_or("handle")
        )),
    }
}

/// The entry-point shapes sharing the runtime dispatch contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// One thread per element over `inputs` buffers.
    Map { inputs: usize },
    /// One thread folding `in0` into the accumulator seeded in `out[0]`.
    Fold,
    /// One thread publishes each inclusive prefix and its final accumulator.
    Scan,
    /// One thread compacts matching elements and publishes the kept count.
    Filter,
}

impl Shape {
    fn has_state(self) -> bool {
        matches!(self, Self::Scan | Self::Filter)
    }
}

/// Install an entry point around `body` and hand back its name and kinds.
fn install(cg: &mut Codegen, body: &DeviceBody, shape: Shape) -> DeviceKernel {
    let name = format!("{KERNEL_PREFIX}{}", cg.device.installed);
    cg.device.installed += 1;
    cg.device
        .source
        .push_str(&entries::entry(&name, body, shape));
    DeviceKernel {
        name,
        kinds: body.uniforms.iter().map(Uniform::kind).collect(),
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
