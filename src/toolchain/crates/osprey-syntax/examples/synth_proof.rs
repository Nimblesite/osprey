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
            | "satAdd"
            | "toFloat"
            | "sin"
            | "cos"
            | "listPrepend"
    ) {
        return Err(format!("binding shadows a reflected operation: {name}"));
    }
    ident(name)
}

fn pattern(p: &Pattern) -> Result<String, String> {
    match p {
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
        _ => Err("unsupported pattern".into()),
    }
}

fn callee(e: &Expr) -> Result<String, String> {
    let Expr::Identifier(name) = e else {
        if let Expr::Path(path) = e {
            return match path.to_string().as_str() {
                "SpectralMath::twiddle" => Ok("ops.twiddle".into()),
                "SpectralMath::productRe" => Ok("ops.productRe".into()),
                "SpectralMath::productIm" => Ok("ops.productIm".into()),
                "SpectralMath::unitRe" => Ok("ops.unitRe".into()),
                "SpectralMath::unitIm" => Ok("ops.unitIm".into()),
                other => Err(format!("unsupported qualified call {other}")),
            };
        }
        return Err("unsupported callee".into());
    };
    match name.as_str() {
        "added" | "subtracted" | "rotated" | "combined" => Ok(format!("{} ops", ident(name)?)),
        "satAdd" | "toFloat" | "sin" | "cos" => Ok(format!("ops.{name}")),
        "listPrepend" => Ok("(fun tail head => List.cons head tail)".into()),
        _ => Err(format!("unsupported callee {name}")),
    }
}

fn term(e: &Expr) -> Result<String, String> {
    match e {
        Expr::Identifier(n) => ident(n),
        Expr::Integer(n) if *n >= 0 => Ok(n.to_string()),
        Expr::Float(n)
            if [0.0_f64, 1.0, 2.0, 4.0]
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
            let name = match op.as_str() {
                "+" => "plus",
                "*" => "times",
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
        Expr::TypeConstructor {
            name,
            type_args,
            fields,
        } if type_args.is_empty() => {
            let record = match name.as_str() {
                "Complex" => "Bin",
                "ComplexHalves" => "Halves",
                _ => return Err(format!("unsupported constructor {name}")),
            };
            let assignments = fields
                .iter()
                .map(|f| Ok(format!("{} := {}", ident(&f.name)?, term(&f.value)?)))
                .collect::<Result<Vec<_>, String>>()?;
            Ok(format!("({{ {} }} : {record} α)", assignments.join(", ")))
        }
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
    if namespace.label() != "synth" {
        return Err("expected synth namespace".into());
    }
    let signatures: &[(&str, &[&str], &str)] = match path.to_string().as_str() {
        "SpectralFrames" => &[
            ("scaled", &["Bin α", "α"], "Bin α"),
            ("turned", &["Bin α", "Bin α"], "Bin α"),
            ("rotation", &["Bin α", "Bin α"], "Bin α"),
        ],
        "Ffts" => &[
            ("conjugate", &["Bin α"][..], "Bin α"),
            ("added", &["Bin α", "Bin α"][..], "Bin α"),
            ("subtracted", &["Bin α", "Bin α"][..], "Bin α"),
            ("rotated", &["Bin α", "α"][..], "Bin α"),
            (
                "quartet",
                &["Bin α", "Bin α", "Bin α", "Bin α", "α"][..],
                "List (Bin α)",
            ),
            (
                "combined",
                &["List (Bin α)", "List (Bin α)", "Nat", "Nat", "α"][..],
                "Halves α",
            ),
        ],
        other => return Err(format!("unsupported production module {other}")),
    };
    for &(name, types, result) in signatures {
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
        println!(
            "def {name} {{α : Type}} (ops : Ops α) {} : {result} := {}",
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
