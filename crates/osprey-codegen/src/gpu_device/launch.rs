//! Launch sites [GPU-BACKEND-DEVICE]: every combinator asks the runtime to
//! run its kernel on the selected device first. The answer is 1 when the
//! device completed the work, 0 when the host loop must run; a device failure
//! never returns ([GPU-DEVICE-FAULT]). A kernel without a device form declines
//! by name, which the host honours and a selected device rejects.

use super::{install, DeviceBody, DeviceKernel, Shape, SOURCE_POINTER};
use crate::builder::Codegen;
use crate::iter::Callback;
use crate::llty::{LType, Value};

/// The runtime entry points of the launch contract.
const LAUNCH: &str = "osprey_gpu_launch";
const LAUNCH_PARAMS: &str = "i8*, i8*, i8*, i64*, i8*, i8*, i8*, i64*, i64";
const DECLINED: &str = "osprey_gpu_declined";

/// The device body of `kernel` with the uniforms its launch passes, or the
/// reason it has none.
fn twin(
    cg: &Codegen,
    kernel: &Callback,
    slots: &[LType],
) -> Result<(DeviceBody, Vec<Value>), String> {
    match kernel {
        Callback::Extracted(extracted) => extracted
            .device
            .clone()
            .map(|body| (body, extracted.uniforms.clone())),
        Callback::Named(name) => named(cg, name, slots).map(|body| (body, Vec::new())),
        Callback::Local(name, _) => {
            let target = super::aliased(cg, name);
            if cg.device.defs.contains_key(&target) && cg.lookup(&target).is_none() {
                named(cg, &target, slots).map(|body| (body, Vec::new()))
            } else {
                Err(local_reason(cg, name, &target))
            }
        }
        Callback::Lambda(parameters, body, ..) | Callback::Closed(_, parameters, body, ..) => {
            Err(capture_reason(cg, parameters, body)
                .unwrap_or_else(|| "the kernel keeps its inlined host lowering".into()))
        }
        Callback::Value(..) => Err("a computed closure value has no device form".into()),
    }
}

/// Why a let-bound lambda kept its host lowering: the capture the device
/// cannot carry, when extraction declined over one.
fn local_reason(cg: &Codegen, name: &str, target: &str) -> String {
    let Some((parameters, body, _)) = cg.lambda_def(target) else {
        return format!("`{name}` is a function value whose body the device cannot see");
    };
    capture_reason(cg, parameters, body)
        .unwrap_or_else(|| format!("the lambda `{name}` kept its inlined host lowering"))
}

/// The captures of a lambda kernel that have no device representation, named.
fn capture_reason(
    cg: &Codegen,
    parameters: &[osprey_ast::Parameter],
    body: &osprey_ast::Expr,
) -> Option<String> {
    let blocked: Vec<String> = crate::closure::capture_list(cg, parameters, body)
        .iter()
        .filter(|capture| !crate::gpu_kernel::uniform_admissible(&capture.val))
        .map(|capture| format!("`{}` ({})", capture.name, describe_value(&capture.val)))
        .collect();
    (!blocked.is_empty()).then(|| {
        format!(
            "the kernel captures {}, which has no device representation",
            blocked.join(", ")
        )
    })
}

fn describe_value(value: &Value) -> String {
    match (&value.osp_ty, value.ty) {
        (Some(owner), _) => owner.clone(),
        (None, LType::Str) => "a string".into(),
        (None, _) if value.result_inner.is_some() => "a Result".into(),
        (None, _) => "a handle".into(),
    }
}

fn named(cg: &Codegen, name: &str, slots: &[LType]) -> Result<DeviceBody, String> {
    let (parameters, body) = cg
        .device
        .defs
        .get(name)
        .cloned()
        .ok_or_else(|| format!("`{name}` is not a function the device can compile"))?;
    super::compile(cg, &parameters, &body, slots, &[])
}

/// Launch `kernel` over `inputs` into `out`, one thread per element; the
/// result stays on the device until the host reads it [GPU-BUFFER-RESIDENT].
pub(crate) fn launch_map(
    cg: &mut Codegen,
    what: &str,
    kernel: &Callback,
    slots: &[LType],
    inputs: &[&Value],
    out: &Value,
    len: &str,
) -> String {
    match twin(cg, kernel, slots) {
        Ok((body, uniforms)) => {
            let installed = install(
                cg,
                &body,
                Shape::Map {
                    inputs: inputs.len(),
                },
            );
            let site = Site {
                inputs,
                out: &out.operand,
                acc: "null",
                count: len,
            };
            emit(cg, &installed, &uniforms, &site)
        }
        Err(reason) => declined(cg, &format!("{what} kernel: {reason}")),
    }
}

/// Launch `kernel` as one thread folding `src` into the accumulator slot.
pub(crate) fn launch_fold(
    cg: &mut Codegen,
    kernel: &Callback,
    slots: &[LType],
    src: &Value,
    acc: &str,
    len: &str,
) -> String {
    match twin(cg, kernel, slots) {
        Ok((body, uniforms)) => {
            let installed = install(cg, &body, Shape::Fold);
            let site = Site {
                inputs: &[src],
                out: "null",
                acc,
                count: len,
            };
            emit(cg, &installed, &uniforms, &site)
        }
        Err(reason) => declined(cg, &format!("gpuFold kernel: {reason}")),
    }
}

/// Ask the runtime about a launch with no device form: 0 under the host,
/// never returns under a device.
pub(crate) fn declined(cg: &mut Codegen, what: &str) -> String {
    let reason = cg.string_constant(what);
    cg.call("i32", DECLINED, "i8*", &[&reason.operand])
}

/// The operands of one launch beyond the kernel and its uniforms.
struct Site<'a> {
    inputs: &'a [&'a Value],
    /// A map's result buffer, left on the device; `null` for a fold.
    out: &'a str,
    /// A fold's accumulator slot, written back; `null` for a map.
    acc: &'a str,
    /// Elements processed: the thread count, or the trip count of a fold.
    count: &'a str,
}

fn emit(cg: &mut Codegen, kernel: &DeviceKernel, uniforms: &[Value], site: &Site<'_>) -> String {
    let name = cg.string_constant(&kernel.name).operand;
    let kinds = cg.string_constant(&kernel.kinds).operand;
    let source = cg.emit_reg(format!("load i8*, i8** {SOURCE_POINTER}"));
    let words = uniform_words(cg, uniforms);
    let (in0, in1) = (input(site, 0), input(site, 1));
    let operands = [
        &name, &source, &kinds, &words, &in0, &in1, site.out, site.acc, site.count,
    ];
    cg.call("i32", LAUNCH, LAUNCH_PARAMS, &operands)
}

/// The `i`-th element buffer's handle, `null` when the launch has none there.
fn input(site: &Site<'_>, i: usize) -> String {
    site.inputs
        .get(i)
        .map_or_else(|| "null".to_string(), |value| value.operand.clone())
}

/// The uniforms boxed to words in launch order, as a pointer to the first.
fn uniform_words(cg: &mut Codegen, uniforms: &[Value]) -> String {
    if uniforms.is_empty() {
        return "null".into();
    }
    let n = uniforms.len();
    let array = cg.hoist_slot(&format!("[{n} x i64]"));
    for (i, uniform) in uniforms.iter().enumerate() {
        let word = crate::conv::box_to_i64(cg, uniform.clone()).operand;
        let slot = cg.emit_reg(format!(
            "getelementptr [{n} x i64], [{n} x i64]* {array}, i64 0, i64 {i}"
        ));
        cg.emit(format!("store i64 {word}, i64* {slot}"));
    }
    cg.emit_reg(format!(
        "getelementptr [{n} x i64], [{n} x i64]* {array}, i64 0, i64 0"
    ))
}
