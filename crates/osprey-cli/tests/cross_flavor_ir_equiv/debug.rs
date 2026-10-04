//! Native debugger scope contracts in both source flavors.
use super::{ir_for, parsed_source, Flavor};

const CAPTURED_LAMBDAS: [(&str, Flavor); 2] = [
    ("fn makeAdder(n) = fn(x) => {\n    let sum = wrapAdd(x, n)\n    sum\n}\nfn main() = {\n    let add = makeAdder(2)\n    print(add(40))\n}\n", Flavor::Default),
    ("makeAdder n = \\x =>\n    sum = wrapAdd x n\n    sum\nmain () =\n    add = makeAdder 2\n    print (add 40)\n", Flavor::Ml),
];

/// [DEBUGGER-SOURCE-MAP] Source lambdas need scopes, body lines and variables.
#[test]
fn captured_lambda_bodies_keep_debug_scopes_in_both_flavors() -> Result<(), String> {
    for (source, flavor) in CAPTURED_LAMBDAS {
        let ir = lambda_debug_ir(source, flavor)?;
        for expected in [
            "!DISubprogram(name: \"__closure_fn_",
            "!DILocation(line: 2,",
            "!DILocation(line: 3,",
            "!DILocalVariable(name: \"x\", arg: 2,",
            "!DILocalVariable(name: \"sum\"",
            "!DILocalVariable(name: \"n\"",
        ] {
            assert!(ir.contains(expected), "{flavor}: missing {expected}");
        }
        assert_lambda_variables(&ir, "__closure_fn_", &["x", "n", "sum"])?;
    }
    Ok(())
}

pub(super) fn lambda_debug_ir(source: &str, flavor: Flavor) -> Result<String, String> {
    let program = parsed_source(source, flavor, "lambda.osp")?.program;
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "{flavor}: {errors:?}");
    let ir = osprey_codegen::compile_program_debug(
        &program,
        osprey_codegen::DebugSource::from_path("lambda.osp"),
    )
    .map_err(|error| format!("{flavor}: {error}"))?;
    for definition in ir.lines().filter(|line| line.starts_with("define ")) {
        if let Some((_, attachment)) = definition.split_once("!dbg ") {
            let scope = attachment.trim_end_matches(" {");
            assert!(
                ir.lines()
                    .any(|line| line.starts_with(&format!("{scope} = distinct !DISubprogram("))),
                "function metadata must remain a subprogram: {definition}"
            );
        }
    }
    Ok(ir)
}

pub(super) fn assert_lambda_variables(
    ir: &str,
    name: &str,
    variables: &[&str],
) -> Result<(), String> {
    let prefix = format!("!DISubprogram(name: \"{name}");
    let (scope, _) = ir
        .lines()
        .find(|line| line.contains(&prefix))
        .and_then(|line| line.split_once(" = "))
        .ok_or_else(|| format!("missing lambda scope {name}"))?;
    for variable in variables {
        let owners = ir
            .lines()
            .filter(|line| line.contains(&format!("!DILocalVariable(name: \"{variable}\",")))
            .map(|line| scope_parent(line).and_then(|owner| scope_chain(ir, owner)))
            .collect::<Result<Vec<_>, _>>()?;
        assert!(
            owners.iter().any(|chain| chain.contains(&scope)),
            "{variable} must belong to {name}'s scope {scope}"
        );
    }
    Ok(())
}

/// [DEBUGGER-SOURCE-MAP] Every materialized source-lambda path has a native scope.
#[test]
fn bound_argument_and_ffi_lambdas_keep_their_debug_scopes() -> Result<(), String> {
    let paths = [
        ("fn main() = {\n let f = fn(x) => wrapMul(x, 2)\n print(f(21))\n}", Flavor::Default, "__closure_fn_", 2),
        ("main () =\n    f = \\x => wrapMul x 2\n    print (f 21)", Flavor::Ml, "__closure_fn_", 2),
        ("fn apply(f, n) = f(n)\nfn main() = print(apply(fn(x) => wrapMul(x, 2), 21))", Flavor::Default, "__closure_fn_", 2),
        ("apply f n = f n\nmain () = print (apply (\\x => wrapMul x 2) 21)", Flavor::Ml, "__closure_fn_", 2),
        ("extern fn invoke(f: (int) -> int, n: int) -> int\nfn main() = print(invoke(fn(x) => wrapMul(x, 2), 21))", Flavor::Default, "__callback_", 1),
        ("extern invoke (f : int -> int) (n : int) -> int\nmain () = print (invoke (\\x => wrapMul x 2) 21)", Flavor::Ml, "__callback_", 1),
    ];
    for (source, flavor, name, arg) in paths {
        let ir = lambda_debug_ir(source, flavor)?;
        assert_lambda_variables(&ir, name, &["x"])?;
        assert!(ir.contains(&format!("!DILocalVariable(name: \"x\", arg: {arg},")));
    }
    Ok(())
}

/// [DEBUGGER-BLOCK-SCOPES] Inner bindings have a scope separate from function locals.
#[test]
fn nested_source_blocks_keep_distinct_variable_scopes() -> Result<(), String> {
    for (source, flavor) in [
        ("fn choose(input) = {\n let value = 100\n let selected = {\n  let value = input\n  let observed = wrapAdd(value, 1)\n  observed\n }\n let outside = wrapAdd(value, selected)\n outside\n}\nprint(choose(2))\n", Flavor::Default),
        ("choose input =\n    value = 100\n    selected =\n        value = input\n        observed = wrapAdd value 1\n        observed\n    outside = wrapAdd value selected\n    outside\nprint (choose 2)\n", Flavor::Ml),
    ] {
        let normal = ir_for(source, flavor, "block.osp")?;
        assert!(!normal.contains("br label"), "scope boundaries add no ordinary control flow");
        assert!(!normal.contains("store volatile"), "debug markers stay out of ordinary builds");
        let ir = lambda_debug_ir(source, flavor)?;
        assert_eq!(ir.matches("%__osprey_debug_scope = alloca i8").count(), 1);
        let inside = local_scope(&ir, "observed")?;
        let outside = local_scope(&ir, "outside")?;
        assert_ne!(inside, outside, "{flavor}: nested locals need their own scope");
        let inner_chain = scope_chain(&ir, inside)?;
        let outer_chain = scope_chain(&ir, outside)?;
        let common = local_scope(&ir, "value")?;
        assert!(inner_chain.contains(&common) && outer_chain.contains(&common));
        assert!(!inner_chain.contains(&outside) && !outer_chain.contains(&inside),
            "{flavor}: nested and subsequent declarations must occupy separate scope branches");
        assert!(ir.lines().any(|line| line.contains("!DILocation(line: 6,") && line.contains(&format!("scope: {inside})"))), "{flavor}: the block return must remain inside its scope");
    }
    Ok(())
}

fn local_scope<'a>(ir: &'a str, name: &str) -> Result<&'a str, String> {
    ir.lines()
        .find(|line| line.contains(&format!("!DILocalVariable(name: \"{name}\",")))
        .ok_or_else(|| format!("missing scope for {name}"))
        .and_then(scope_parent)
}

fn scope_parent(metadata: &str) -> Result<&str, String> {
    metadata
        .split_once("scope: ")
        .and_then(|(_, scope)| scope.split(',').next())
        .ok_or_else(|| format!("missing scope parent in {metadata}"))
}

/// Follow verified lexical parents, rejecting missing nodes and cycles.
fn scope_chain<'a>(ir: &'a str, scope: &'a str) -> Result<Vec<&'a str>, String> {
    let mut chain = vec![scope];
    let mut current = scope;
    loop {
        let metadata = ir
            .lines()
            .find(|line| line.starts_with(&format!("{current} = ")))
            .ok_or_else(|| format!("missing scope metadata {current}"))?;
        if metadata.contains("!DISubprogram(") {
            return Ok(chain);
        }
        assert!(
            metadata.contains("!DILexicalBlock("),
            "invalid scope: {metadata}"
        );
        let parent = scope_parent(metadata)?;
        assert!(!chain.contains(&parent), "cyclic scope: {parent}");
        chain.push(parent);
        current = parent;
    }
}

/// [DEBUGGER-BINDING-LIFETIME] Cleanup must not reintroduce an earlier breakpoint.
#[test]
fn top_level_bindings_keep_monotonic_source_locations() -> Result<(), String> {
    for (source, flavor) in [
        ("fn square(n) = wrapMul(n, n)\nfn addThree(n) = wrapAdd(n, 3)\nlet first = square(5)\nlet second = square(7)\nlet third = addThree(second)\nprint(\"${first}:${second}:${third}\")\n", Flavor::Default),
        ("square n = wrapMul n n\naddThree n = wrapAdd n 3\nfirst = square 5\nsecond = square 7\nthird = addThree second\nprint \"${first}:${second}:${third}\"\n", Flavor::Ml),
    ] {
        let ir = lambda_debug_ir(source, flavor)?;
        let lines = function_source_lines(&ir, "main")?;
        assert!(lines.iter().zip(lines.iter().skip(1)).all(|(left, right)| left <= right), "{flavor}: {lines:?}");
        for expected in 3..=6 { assert!(lines.contains(&expected), "{flavor}: lost line {expected}"); }
    }
    Ok(())
}

fn function_source_lines(ir: &str, name: &str) -> Result<Vec<usize>, String> {
    let body = ir
        .lines()
        .skip_while(|line| !line.starts_with("define ") || !line.contains(&format!("@{name}(")))
        .take_while(|line| *line != "}");
    body.filter_map(|line| {
        line.split_once("!dbg ")
            .map(|(_, id)| id.trim_end_matches(" {"))
    })
    .filter_map(|id| {
        ir.lines()
            .find(|line| line.starts_with(&format!("{id} = !DILocation(")))
    })
    .map(|line| {
        line.split_once("line: ")
            .and_then(|(_, tail)| tail.split(',').next())
            .ok_or_else(|| format!("missing source line: {line}"))?
            .parse::<usize>()
            .map_err(|error| error.to_string())
    })
    .collect()
}
