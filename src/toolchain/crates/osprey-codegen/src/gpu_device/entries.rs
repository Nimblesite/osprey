//! Typed GPU entry points and sequential shapes [GPU-DEVICE-NATIVE].
use super::{numeric, DeviceBody, Shape, Ty, Uniform};
use std::fmt::Write as _;

/// The fixed leading parameters every entry point takes, which the runtime
/// binds first: the element buffers, the result, the fault word, the count.
fn fixed_parameters(body: &DeviceBody, shape: Shape) -> String {
    let offset = usize::from(matches!(shape, Shape::Fold | Shape::Scan));
    let input = |index| {
        body.param_storage
            .get(index + offset)
            .copied()
            .unwrap_or(Ty::INT)
            .spelling()
    };
    let output = output_type(body, shape).spelling();
    format!("GPU_GLOBAL const {} *in0 GPU_BUFFER(0), GPU_GLOBAL const {} *in1 GPU_BUFFER(1), \
GPU_GLOBAL {output} *out GPU_BUFFER(2), GPU_GLOBAL ulong *fault GPU_BUFFER(3), GPU_PARAM count GPU_BUFFER(4)", input(0), input(1))
}

fn output_type(body: &DeviceBody, shape: Shape) -> Ty {
    if shape == Shape::Filter {
        body.param_storage.first().copied().unwrap_or(Ty::INT)
    } else {
        body.result_storage
    }
}

fn scan_entry(text: &mut String, body: &DeviceBody) {
    let close = format!(
        "    out[i] = {};\n  }}\n  state[0] = {};",
        body.result_storage.word_of("acc"),
        numeric::state_word(body.result_storage, "acc")
    );
    let start = numeric::uniform_value(body.result_storage, "state[0]");
    accumulating_entry(text, body, &start, &close);
}

fn filter_entry(text: &mut String, body: &DeviceBody) {
    let elem = body.params.first().copied().unwrap_or(Ty::INT);
    let elem_storage = body.param_storage.first().copied().unwrap_or(Ty::INT);
    let _ = writeln!(text, "  if (GPU_INDEX != 0) return;\n  ulong kept = 0;\n  for (ulong i = 0; i < count; i++) {{\n    {} p0 = {};", elem.spelling(), elem_storage.value_of("in0[i]"));
    indented(text, &body.lines, "    ");
    let _ = writeln!(
        text,
        "    if ({}) out[kept++] = {};\n  }}\n  state[0] = kept;",
        body.result,
        elem_storage.word_of("p0")
    );
}

pub(super) fn entry(name: &str, body: &DeviceBody, shape: Shape) -> String {
    let mut text = format!(
        "kernel void {name}({}{} GPU_THREAD) {{\n",
        fixed_parameters(body, shape),
        uniform_parameters(body, shape)
    );
    text.push_str(&uniform_bindings(body));
    match shape {
        Shape::Map { inputs } => map_entry(&mut text, body, inputs),
        Shape::Fold => fold_entry(&mut text, body),
        Shape::Scan => scan_entry(&mut text, body),
        Shape::Filter => filter_entry(&mut text, body),
    }
    text.push_str("}\n");
    text
}

fn uniform_parameters(body: &DeviceBody, shape: Shape) -> String {
    let mut index = if shape.has_state() { 6 } else { 5 };
    let mut text = if shape.has_state() {
        ", GPU_GLOBAL ulong *state GPU_BUFFER(5)".to_owned()
    } else {
        String::new()
    };
    for (i, uniform) in body.uniforms.iter().enumerate() {
        match uniform {
            Uniform::Scalar(_) => {
                let _ = write!(text, ", GPU_PARAM u{i} GPU_BUFFER({index})");
                index += 1;
            }
            Uniform::Buffer(ty) => {
                let _ = write!(
                    text,
                    ", GPU_GLOBAL const {} *u{i} GPU_BUFFER({index}), GPU_PARAM u{i}_len GPU_BUFFER({})", ty.spelling(),
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
                ty.compute_spelling(),
                numeric::uniform_value(*ty, &format!("u{i}"))
            )),
            Uniform::Buffer(_) => None,
        })
        .collect()
}

fn map_entry(text: &mut String, body: &DeviceBody, inputs: usize) {
    text.push_str("  ulong gid = GPU_INDEX;\n  if (gid >= count) return;\n");
    for (i, ty) in body.params.iter().enumerate().take(inputs) {
        let storage = body.param_storage.get(i).copied().unwrap_or(*ty);
        let _ = writeln!(
            text,
            "  {} p{i} = {};",
            ty.spelling(),
            storage.value_of(&format!("in{i}[gid]"))
        );
    }
    indented(text, &body.lines, "  ");
    let _ = writeln!(
        text,
        "  out[gid] = {};",
        body.result_storage.word_of(&body.result)
    );
}

fn fold_entry(text: &mut String, body: &DeviceBody) {
    let close = format!("  }}\n  out[0] = {};", body.result_storage.word_of("acc"));
    let start = body.result_storage.value_of("out[0]");
    accumulating_entry(text, body, &start, &close);
}

// The single-device loop scan and fold share; only the start and the close differ.
fn accumulating_entry(text: &mut String, body: &DeviceBody, start: &str, close: &str) {
    let acc = body.params.first().copied().unwrap_or(Ty::INT);
    let elem = body.params.get(1).copied().unwrap_or(Ty::INT);
    let elem_storage = body.param_storage.get(1).copied().unwrap_or(Ty::INT);
    let _ = writeln!(
        text,
        "  if (GPU_INDEX != 0) return;\n  {} acc = {start};\n  for (ulong i = 0; i < count; i++) {{",
        acc.spelling()
    );
    let _ = writeln!(
        text,
        "    {} p0 = acc;\n    {} p1 = {};",
        acc.spelling(),
        elem.spelling(),
        elem_storage.value_of("in0[i]")
    );
    indented(text, &body.lines, "    ");
    let _ = writeln!(text, "    acc = {};\n{close}", body.result);
}

fn indented(text: &mut String, lines: &[String], indent: &str) {
    for line in lines {
        let _ = writeln!(text, "{indent}{line}");
    }
}
