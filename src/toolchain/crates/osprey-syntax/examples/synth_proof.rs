//! Reflect the production FFT's pure ML bodies into the synthesis proof environment.
#[path = "synth_proof/signatures.rs"]
mod signatures;
use osprey_ast::{Expr, Pattern, Stmt};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use signatures::signatures;
use std::{env, fs};

fn ident(name: &str) -> Result<String, String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(format!("unsupported identifier {name}"));
    }
    Ok(format!("«{name}»"))
}

fn binder(name: &str, module: &str) -> Result<String, String> {
    if (module == "Ffts" && name == "sizes")
        || matches!(name, "ops" | "recur")
        || primitive(name).is_some()
        || signatures(module)?
            .iter()
            .any(|(member, _, _)| *member == name)
    {
        return Err(format!("binding shadows a reflected operation: {name}"));
    }
    ident(name)
}

fn pattern(p: &Pattern, module: &str) -> Result<String, String> {
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
            [Pattern::Binding(head)] => Ok(format!(
                "{} :: {}",
                binder(head, module)?,
                binder(tail, module)?
            )),
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
                    Pattern::Binding(n) => binder(n, module),
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
            Ok(format!("{ctor} {}", binder(n, module)?))
        }
        _ => Err(format!("unsupported pattern {p:?}")),
    }
}

fn callee(e: &Expr, module: &str) -> Result<String, String> {
    let Expr::Identifier(name) = e else {
        if let Expr::Path(path) = e {
            if let [module, op] = path.segments.as_slice() {
                if module == "NonlinearMath"
                    && signatures(module)?
                        .iter()
                        .any(|(member, _, _)| member == op)
                {
                    return Ok(format!("NonlinearMath.{} ops", ident(op)?));
                }
            }
            return match path.to_string().as_str() {
                "FitMath::clip" => Ok("FitMath.clip ops".into()),
                "GrainMath::asr" => Ok("GrainMath.asr ops".into()),
                "PhaseMath::window" => Ok("PhaseMath.window ops".into()),
                "PhaseMath::windowedPhase" => Ok("PhaseMath.windowedPhase ops".into()),
                "DelayLines::push" => Ok("DelayLines.push ops".into()),
                "DelayLines::tap" => Ok("ops.tap".into()),
                "TableMath::harmonic" => Ok("TableMath.harmonic ops".into()),
                "SpectralMath::twiddle" => Ok("SpectralMath.twiddle ops".into()),
                "SpectralMath::productRe" => Ok("SpectralMath.productRe ops".into()),
                "SpectralMath::productIm" => Ok("SpectralMath.productIm ops".into()),
                "SpectralMath::unitRe" => Ok("SpectralMath.unitRe ops".into()),
                "SpectralMath::unitIm" => Ok("SpectralMath.unitIm ops".into()),
                other => Err(format!("unsupported qualified call {other}")),
            };
        }
        return Err("unsupported callee".into());
    };
    if let Some(builtin) = primitive(name) {
        return Ok(builtin.into());
    }
    if !signatures(module)?
        .iter()
        .any(|(member, _, _)| member == name)
    {
        return Err(format!("unreflected local call {module}.{name}"));
    }
    if module == "Ffts" && name == "transform" {
        return Ok("recur".into());
    }
    Ok(format!("{module}.{} ops", ident(name)?))
}

fn primitive(name: &str) -> Option<&'static str> {
    Some(match name {
        "satAdd" => "ops.satAdd",
        "toFloat" => "ops.toFloat",
        "exp" => "ops.exp",
        "ln" => "ops.ln",
        "pow" => "ops.pow",
        "sin" => "ops.sin",
        "cos" => "ops.cos",
        "listGet" => "readBin",
        "listContains" => "List.contains",
        "intDiv" => "Nat.div",
        "listConcat" => "List.append",
        "foldList" => "foldList",
        "floatDiv" => "ops.ieeeDivide",
        "abs" => "ops.abs",
        "listAppend" => "(fun tail item => tail ++ [item])",
        "listPrepend" => "(fun tail head => List.cons head tail)",
        _ => return None,
    })
}

fn constructor(
    module: &str,
    name: &str,
    fields: &[osprey_ast::FieldAssignment],
) -> Result<String, String> {
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
        return Ok(format!("({ctor} {})", term(&payload.value, module)?));
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
        .map(|f| {
            Ok(format!(
                "{} := {}",
                ident(&f.name)?,
                term(&f.value, module)?
            ))
        })
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

fn binary_operator(op: &str, right: &Expr) -> Result<&'static str, String> {
    match op {
        "==" if matches!(right, Expr::Integer(_)) => Ok("BEq.beq (α := Nat)"),
        "||" => Ok("or"),
        "<=" => Ok("ops.atMost"),
        ">=" => Ok("ops.atLeast"),
        "+" => Ok("ops.plus"),
        "*" => Ok("ops.times"),
        "<" => Ok("ops.less"),
        ">" => Ok("ops.greater"),
        "==" => Ok("ops.equal"),
        "/" => Ok("ops.divide"),
        "-" => Ok("ops.minus"),
        _ => Err(format!("unsupported operator {op}")),
    }
}

fn block(statements: &[Stmt], value: &Expr, module: &str) -> Result<String, String> {
    let mut body = term(value, module)?;
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
        body = format!(
            "(let {} := {}; {body})",
            binder(name, module)?,
            term(value, module)?
        );
    }
    Ok(body)
}

fn float_literal(n: f64) -> String {
    let bits = n.to_bits();
    for (value, op) in [
        (std::f64::consts::PI, "ops.pi"),
        (std::f64::consts::TAU, "ops.tau"),
        (0.5, "ops.half"),
        (0.00001, "ops.epsilon"),
    ] {
        if bits == value.to_bits() {
            return op.into();
        }
    }
    if [0.0_f64, 1.0, 2.0, 3.0, 4.0, 8.0]
        .iter()
        .any(|v| bits == v.to_bits())
    {
        return format!("(ops.literal {n:.0})");
    }
    format!("(ops.binary64 0x{bits:016x})")
}

fn term(e: &Expr, module: &str) -> Result<String, String> {
    match e {
        Expr::Identifier(n) if module == "Ffts" && n == "sizes" => {
            Ok("(Ffts.sizes.map Int.ofNat)".into())
        }
        Expr::Identifier(n) => ident(n),
        Expr::Integer(n) if *n >= 0 => Ok(n.to_string()),
        Expr::Float(n) => Ok(float_literal(*n)),
        Expr::List(items, _) => Ok(format!(
            "[{}]",
            items
                .iter()
                .map(|item| term(item, module))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        Expr::FieldAccess { target, field } => {
            Ok(format!("({}).{}", term(target, module)?, ident(field)?))
        }
        Expr::Binary {
            op, left, right, ..
        } => {
            let operator = binary_operator(op, right)?;
            Ok(format!(
                "({operator} {} {})",
                term(left, module)?,
                term(right, module)?
            ))
        }
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } if named_arguments.is_empty() => {
            if matches!(function.as_ref(), Expr::Identifier(n) if n == "intDiv")
                && !matches!(arguments.as_slice(), [_, Expr::Integer(2)])
            {
                return Err("only nonnegative frame-size halving is supported".into());
            }
            let args = arguments
                .iter()
                .map(|item| term(item, module))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!(
                "({} {})",
                callee(function, module)?,
                args.join(" ")
            ))
        }
        Expr::Lambda {
            parameters,
            return_type: None,
            body,
            ..
        } => {
            let params = parameters
                .iter()
                .map(|p| binder(&p.name, module))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!(
                "(fun {} => {})",
                params.join(" "),
                term(body, module)?
            ))
        }
        Expr::TypeConstructor {
            name,
            type_args,
            fields,
        } if type_args.is_empty() => constructor(module, name, fields),
        Expr::Block {
            statements,
            value: Some(value),
            ..
        } => block(statements, value, module),
        Expr::Match { value, arms } => {
            let cases = arms
                .iter()
                .map(|a| {
                    Ok(format!(
                        "| {} => {}",
                        pattern(&a.pattern, module)?,
                        term(&a.body, module)?
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(format!(
                "(match {} with {})",
                term(value, module)?,
                cases.join(" ")
            ))
        }
        _ => Err(format!("unsupported expression {e:?}")),
    }
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
    for item in body {
        match item.declaration.as_ref() {
            Stmt::Function { name, .. } | Stmt::Let { name, .. } | Stmt::Extern { name, .. }
                if primitive(name).is_some() =>
            {
                return Err(format!(
                    "module declaration shadows a reflected primitive: {module}.{name}"
                ));
            }
            _ => {}
        }
    }
    if module == "Ffts" {
        let mut sizes = body
            .iter()
            .filter_map(|item| match item.declaration.as_ref() {
                Stmt::Let {
                    name,
                    mutable: false,
                    value,
                    ..
                } if name == "sizes" => Some(value),
                _ => None,
            });
        let value = sizes.next().ok_or("missing immutable Ffts.sizes")?;
        if sizes.next().is_some() {
            return Err("ambiguous Ffts.sizes".into());
        }
        println!("def Ffts.sizes : List Nat := {}", term(value, &module)?);
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
            .map(|(p, t)| Ok(format!("({} : {t})", binder(&p.name, &module)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let ops_name = if matches!(
            (module.as_str(), name),
            ("DelayLines", "push") | ("Ffts", "valid")
        ) {
            "_ops"
        } else {
            "ops"
        };
        // Interpret recursive call sites explicitly so finite evaluation can
        // quantify over its depth and prove that the exhaustion case is unreachable.
        let (definition, recursion) = if module == "Ffts" && name == "transform" {
            ("transformStep", "(recur : Transform α ops.Fault)")
        } else {
            (name, "")
        };
        println!(
            "def {module}.{definition} {{α : Type}} ({ops_name} : Ops α) {recursion} {} : {result} := {}",
            params.join(" "),
            term(expr, &module)?
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
