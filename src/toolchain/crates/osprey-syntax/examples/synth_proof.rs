//! Reflect the production FFT's pure ML bodies into the synthesis proof environment.
use osprey_ast::{Expr, Pattern, Stmt};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::{env, fs};

fn ident(name: &str) -> Result<String, String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(format!("unsupported identifier {name}"));
    }
    Ok(format!("«{name}»"))
}

fn binder(name: &str) -> Result<String, String> {
    if matches!(
        name,
        "ops"
            | "added"
            | "subtracted"
            | "rotated"
            | "combined"
            | "listGet"
            | "incomplete"
            | "quartet"
            | "gated"
            | "foldList"
            | "floatDiv"
            | "satAdd"
            | "toFloat"
            | "sin"
            | "cos"
            | "listAppend"
            | "listPrepend"
            | "curve"
            | "integral"
            | "abs"
            | "hann"
            | "basePhase"
            | "pulse"
            | "delay"
    ) {
        return Err(format!("binding shadows a reflected operation: {name}"));
    }
    ident(name)
}

fn pattern(p: &Pattern) -> Result<String, String> {
    match p {
        Pattern::Literal(v) => match v.as_ref() {
            Expr::Bool(b) => Ok(b.to_string()),
            _ => Err("unsupported literal pattern".into()),
        },
        Pattern::Binding(name)
            if matches!(
                name.as_str(),
                "TanhCurve" | "HardCurve" | "CubicCurve" | "FoldCurve"
            ) =>
        {
            Ok(format!("Curve.{name}"))
        }
        Pattern::List {
            elements,
            rest: None,
        } if elements.is_empty() => Ok("[]".into()),
        Pattern::List {
            elements,
            rest: Some(tail),
        } => match elements.as_slice() {
            [Pattern::Binding(head)] => Ok(format!("{} :: {}", binder(head)?, binder(tail)?)),
            _ => Err("unsupported list head".into()),
        },
        Pattern::Constructor {
            name,
            fields,
            sub_patterns,
        } if fields.is_empty()
            && matches!(
                name.as_str(),
                "HannWindow"
                    | "AsrWindow"
                    | "PlainPhase"
                    | "WindowedPhase"
                    | "DistortedPhase"
                    | "WavePulsePhase"
            ) =>
        {
            let args = sub_patterns
                .iter()
                .map(|p| match p {
                    Pattern::Binding(n) => binder(n),
                    _ => Err("expected phase field binder".into()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let ty = if matches!(name.as_str(), "HannWindow" | "AsrWindow") {
                "Envelope"
            } else {
                "PhaseShape"
            };
            Ok(format!("{ty}.{name} {}", args.join(" ")))
        }
        Pattern::Constructor {
            name,
            fields,
            sub_patterns,
        } if sub_patterns.is_empty() => {
            if fields.is_empty()
                && matches!(
                    name.as_str(),
                    "TanhCurve" | "HardCurve" | "CubicCurve" | "FoldCurve"
                )
            {
                return Ok(format!("Curve.{name}"));
            }
            let ctor = match name.as_str() {
                "Error" => "Except.error",
                "Success" => "Except.ok",
                _ => return Err(format!("unsupported pattern {name}")),
            };
            let [n] = fields.as_slice() else {
                return Err("expected unary constructor pattern".into());
            };
            Ok(format!("{ctor} {}", binder(n)?))
        }
        _ => Err(format!("unsupported pattern {p:?}")),
    }
}

fn callee(e: &Expr) -> Result<String, String> {
    let Expr::Identifier(name) = e else {
        if let Expr::Path(path) = e {
            if let [module, op] = path.segments.as_slice() {
                if module == "NonlinearMath"
                    && matches!(
                        op.as_str(),
                        "tanh"
                            | "hard"
                            | "cubic"
                            | "fold"
                            | "tanhIntegral"
                            | "hardIntegral"
                            | "cubicIntegral"
                            | "foldIntegral"
                    )
                {
                    return Ok(format!("ops.{op}"));
                }
            }
            return match path.to_string().as_str() {
                "FitMath::clip" => Ok("FitMath.clip ops".into()),
                "GrainMath::asr" => Ok("ops.asr".into()),
                "PhaseMath::window" => Ok("ops.window".into()),
                "PhaseMath::windowedPhase" => Ok("ops.windowedPhase".into()),
                "DelayLines::push" => Ok("DelayLines.push ops".into()),
                "DelayLines::tap" => Ok("ops.tap".into()),
                "TableMath::harmonic" => Ok("TableMath.harmonic ops".into()),
                "SpectralMath::twiddle" => Ok("SpectralMath.twiddle ops".into()),
                "SpectralMath::productRe" => Ok("SpectralMath.productRe ops".into()),
                "SpectralMath::productIm" => Ok("SpectralMath.productIm ops".into()),
                "SpectralMath::unitRe" => Ok("ops.unitRe".into()),
                "SpectralMath::unitIm" => Ok("ops.unitIm".into()),
                other => Err(format!("unsupported qualified call {other}")),
            };
        }
        return Err("unsupported callee".into());
    };
    match name.as_str() {
        "added" | "subtracted" | "rotated" | "combined" | "delay" => {
            let module = if name == "delay" {
                "StrikeCombs"
            } else {
                "Ffts"
            };
            Ok(format!("{module}.{} ops", ident(name)?))
        }
        "satAdd" | "toFloat" | "sin" | "cos" => Ok(format!("ops.{name}")),
        "curve" | "integral" => Ok(format!("Waveshapers.{} ops", ident(name)?)),
        "basePhase" | "pulse" => Ok(format!("PhaseOperators.{} ops", ident(name)?)),
        "hann" => Ok("GrainWindows.hann ops".into()),
        "gated" => Ok("ArrangeMath.gated ops".into()),
        "listGet" => Ok("readBin".into()),
        "incomplete" | "quartet" => Ok(format!("Ffts.{} ops", ident(name)?)),
        "foldList" => Ok("foldList".into()),
        "floatDiv" => Ok("ops.ieeeDivide".into()),
        "abs" => Ok("ops.abs".into()),
        "listAppend" => Ok("(fun tail item => tail ++ [item])".into()),
        "listPrepend" => Ok("(fun tail head => List.cons head tail)".into()),
        _ => Err(format!("unsupported callee {name}")),
    }
}

fn constructor(name: &str, fields: &[osprey_ast::FieldAssignment]) -> Result<String, String> {
    if let Some((ctor, field)) = match name {
        "Error" => Some(("Except.error", "message")),
        "Success" => Some(("Except.ok", "value")),
        _ => None,
    } {
        let [payload] = fields else {
            return Err(format!("invalid {name} fields"));
        };
        if payload.name != field {
            return Err(format!("invalid {name} field {}", payload.name));
        }
        return Ok(format!("({ctor} {})", term(&payload.value)?));
    }
    if name == "InvalidConfiguration" {
        let [field] = fields else {
            return Err("expected one configuration detail".into());
        };
        if field.name == "detail"
            && field.value == Expr::Str("FFT input frame is incomplete.".into())
        {
            return Ok("ops.incompleteFrame".into());
        }
        return Err("unknown configuration error".into());
    }
    let record = match name {
        "LineSample" => "LineSample",
        "AllpassState" => "AllpassState",
        "AllpassSample" => "AllpassSample",
        "Complex" => "Bin",
        "ComplexHalves" => "Halves",
        "DelayLine" => "DelayLine",
        _ => return Err(format!("unsupported constructor {name}")),
    };
    let assignments = fields
        .iter()
        .map(|f| Ok(format!("{} := {}", ident(&f.name)?, term(&f.value)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let args = if record == "LineSample" {
        "α (DelayLine α)"
    } else {
        "α"
    };
    Ok(format!(
        "({{ {} }} : {record} {args})",
        assignments.join(", ")
    ))
}

fn term(e: &Expr) -> Result<String, String> {
    match e {
        Expr::Identifier(n) => ident(n),
        Expr::Integer(n) if *n >= 0 => Ok(n.to_string()),
        Expr::Float(n) if n.to_bits() == std::f64::consts::TAU.to_bits() => Ok("ops.tau".into()),
        Expr::Float(n) if n.to_bits() == 0.5_f64.to_bits() => Ok("ops.half".into()),
        Expr::Float(n) if n.to_bits() == 0.00001_f64.to_bits() => Ok("ops.epsilon".into()),
        Expr::Float(n)
            if [0.0_f64, 1.0, 2.0, 3.0, 4.0, 8.0]
                .iter()
                .any(|v| n.to_bits() == v.to_bits()) =>
        {
            Ok(format!("(ops.literal {n:.0})"))
        }
        Expr::List(items, _) => Ok(format!(
            "[{}]",
            items
                .iter()
                .map(term)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        Expr::FieldAccess { target, field } => Ok(format!("({}).{}", term(target)?, ident(field)?)),
        Expr::Binary {
            op, left, right, ..
        } => {
            if op == "||" {
                return Ok(format!("(or {} {})", term(left)?, term(right)?));
            }
            let name = match op.as_str() {
                "<=" => "atMost",
                ">=" => "atLeast",
                "+" => "plus",
                "*" => "times",
                "<" => "less",
                ">" => "greater",
                "==" => "equal",
                "/" => "divide",
                "-" => "minus",
                _ => return Err(format!("unsupported operator {op}")),
            };
            Ok(format!("(ops.{name} {} {})", term(left)?, term(right)?))
        }
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } if named_arguments.is_empty() => {
            let args = arguments.iter().map(term).collect::<Result<Vec<_>, _>>()?;
            Ok(format!("({} {})", callee(function)?, args.join(" ")))
        }
        Expr::Lambda {
            parameters,
            return_type: None,
            body,
            ..
        } => {
            let params = parameters
                .iter()
                .map(|p| binder(&p.name))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("(fun {} => {})", params.join(" "), term(body)?))
        }
        Expr::TypeConstructor {
            name,
            type_args,
            fields,
        } if type_args.is_empty() => constructor(name, fields),
        Expr::Block {
            statements,
            value: Some(value),
            ..
        } => {
            let mut body = term(value)?;
            for stmt in statements.iter().rev() {
                let Stmt::Let {
                    name,
                    mutable: false,
                    value,
                    ..
                } = stmt
                else {
                    return Err("unsupported block statement".into());
                };
                body = format!("(let {} := {}; {body})", binder(name)?, term(value)?);
            }
            Ok(body)
        }
        Expr::Match { value, arms } => {
            let cases = arms
                .iter()
                .map(|a| Ok(format!("| {} => {}", pattern(&a.pattern)?, term(&a.body)?)))
                .collect::<Result<Vec<_>, String>>()?;
            Ok(format!("(match {} with {})", term(value)?, cases.join(" ")))
        }
        _ => Err(format!("unsupported expression {e:?}")),
    }
}

type Signature = (&'static str, &'static [&'static str], &'static str);

fn signatures(module: &str) -> Result<&'static [Signature], String> {
    Ok(match module {
        "CanvasGeometry" => &[("offset", &["α", "α"], "α"), ("shifted", &["α", "α"], "α")],
        "DialMath" => &[("moved", &["α", "α"], "α")],
        "FitMath" => &[("clip", &["α", "α", "α"], "α")],
        "TimelineGeometry" => &[("heardSeconds", &["α", "α", "α", "α", "α"], "α")],
        "DelayLines" => &[("push", &["DelayLine α", "α"], "DelayLine α")],
        "ArrangeMath" => &[
            ("gated", &["α", "α", "α", "α"], "α"),
            ("enveloped", &["α", "α", "α", "α", "α", "α"], "α"),
        ],
        "PhaseOperators" => &[
            ("basePhase", &["α", "α"], "α"),
            ("pulse", &["α", "Pulse α"], "α"),
            ("shaped", &["PhaseShape α", "α"], "α"),
        ],
        "GrainWindows" => &[
            ("hann", &["α", "α"], "α"),
            ("value", &["Envelope α", "Nat", "Nat"], "α"),
        ],
        "Interpolation" => &[("linear", &["α", "α", "α"], "α")],
        "Waveshapers" => &[
            ("curve", &["Curve", "α"], "α"),
            ("integral", &["Curve", "α"], "α"),
            ("shaped", &["Shaper", "α", "α"], "α"),
        ],
        "Allpasses" => &[("next", &["α", "AllpassState α", "α"], "AllpassSample α")],
        "StrikeCombs" => &[
            ("delay", &["Strike α"], "α"),
            (
                "next",
                &["Strike α", "DelayLine α", "α"],
                "Except ops.Fault (LineSample α (DelayLine α))",
            ),
        ],
        "SpectralStreams" => &[
            ("overlapScale", &["α", "α"], "α"),
            ("overlapped", &["α", "α", "α"], "α"),
        ],
        "TableMath" => &[("harmonic", &["α", "α", "α", "α"], "α")],
        "Wavetables" => &[("value", &["List (Harmonic α)", "α", "α"], "α")],
        "SpectralMath" => &[
            ("hann", &["α", "α"], "α"),
            ("twiddle", &["α", "α", "α"], "α"),
            ("productRe", &["α", "α", "α", "α"], "α"),
            ("productIm", &["α", "α", "α", "α"], "α"),
        ],
        "SpectralFrames" => &[
            ("scaled", &["Bin α", "α"], "Bin α"),
            ("turned", &["Bin α", "Bin α"], "Bin α"),
            ("rotation", &["Bin α", "Bin α"], "Bin α"),
        ],
        "Ffts" => &[
            ("incomplete", &[], "Except ops.Fault (List (Bin α))"),
            ("conjugate", &["Bin α"], "Bin α"),
            ("added", &["Bin α", "Bin α"], "Bin α"),
            ("subtracted", &["Bin α", "Bin α"], "Bin α"),
            ("rotated", &["Bin α", "α"], "Bin α"),
            (
                "quartet",
                &["Bin α", "Bin α", "Bin α", "Bin α", "α"],
                "List (Bin α)",
            ),
            (
                "four",
                &["List (Bin α)", "Nat", "Nat", "α"],
                "Except ops.Fault (List (Bin α))",
            ),
            (
                "combined",
                &["List (Bin α)", "List (Bin α)", "Nat", "Nat", "α"],
                "Halves α",
            ),
        ],
        other => return Err(format!("unsupported production module {other}")),
    })
}

fn reflect(file: &str) -> Result<(), String> {
    let source = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let parsed = parse_program_with_flavor(&source, Flavor::Ml);
    if !parsed.errors.is_empty() {
        return Err(format!("{:?}", parsed.errors));
    }
    let [Stmt::Namespace {
        name: namespace,
        body,
        ..
    }] = parsed.program.statements.as_slice()
    else {
        return Err("expected namespace".into());
    };
    let [Stmt::Module { path, body, .. }] = body.as_slice() else {
        return Err("expected one module".into());
    };
    let module = path.to_string();
    let owner = if module == "ArrangeMath" {
        "daw"
    } else {
        "synth"
    };
    if namespace.label() != owner {
        return Err(format!("expected {owner} namespace for {module}"));
    }
    for &(name, types, result) in signatures(&module)? {
        let mut functions = body
            .iter()
            .filter_map(|item| match item.declaration.as_ref() {
                Stmt::Function {
                    name: n,
                    parameters,
                    body,
                    ..
                } if n == name => Some((parameters, body)),
                _ => None,
            });
        let (parameters, expr) = functions.next().ok_or_else(|| format!("missing {name}"))?;
        if functions.next().is_some() || parameters.len() != types.len() {
            return Err(format!("ambiguous or mismatched signature for {name}"));
        }
        let params = parameters
            .iter()
            .zip(types)
            .map(|(p, t)| Ok(format!("({} : {t})", binder(&p.name)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let ops_name = if module == "DelayLines" && name == "push" {
            "_ops"
        } else {
            "ops"
        };
        println!(
            "def {module}.{name} {{α : Type}} ({ops_name} : Ops α) {} : {result} := {}",
            params.join(" "),
            term(expr)?
        );
    }
    Ok(())
}

fn main() -> Result<(), String> {
    for file in env::args().skip(1) {
        reflect(&file)?;
    }
    Ok(())
}
