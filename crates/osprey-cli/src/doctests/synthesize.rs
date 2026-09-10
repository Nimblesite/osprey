//! Compile each example in its declaration's lexical scope, with a fresh entry.
//! Implements [DOC-DOCTEST-HARNESS].

use osprey_ast::{Expr, ModuleItem, Program, Stmt, SymbolPath, Visibility};
use osprey_project::{ProjectConfig, SourceFile};

const RUNNER_ROLE: &str = "documentation_example";

pub(super) fn program(
    sources: &[SourceFile],
    config: Option<&ProjectConfig>,
    source_index: usize,
    scope: &[usize],
    snippet: Program,
) -> Result<Program, String> {
    let mut sources = sources.to_vec();
    let source = sources
        .get_mut(source_index)
        .ok_or("documentation source is missing")?;
    let call = inject(&mut source.program.statements, scope, snippet)?;
    for source in &mut sources {
        discard_entry(&mut source.program.statements);
    }
    sources.get_mut(source_index).ok_or("documentation source is missing")?
        .program
        .statements
        .push(expression(call));
    assemble(&sources, config, source_index)
}

pub(super) fn reachable(mut program: Program) -> Program {
    let roots: Vec<_> = program
        .statements
        .iter()
        .filter(|stmt| !matches!(stmt, Stmt::Function { .. }))
        .collect();
    let mut names = std::collections::BTreeSet::new();
    osprey_ast::freevars::free_idents_of_stmts(&roots, None, &mut names);
    loop {
        let before = names.len();
        for statement in &program.statements {
            if let Stmt::Function { name, body, .. } = statement {
                if names.contains(name) {
                    osprey_ast::freevars::free_idents(body, &mut names);
                }
            }
        }
        if before == names.len() {
            break;
        }
    }
    program
        .statements
        .retain(|stmt| !matches!(stmt, Stmt::Function { name, .. } if !names.contains(name)));
    program
}

fn assemble(
    sources: &[SourceFile],
    config: Option<&ProjectConfig>,
    index: usize,
) -> Result<Program, String> {
    let source = sources.get(index).ok_or("documentation source is missing")?;
    if config.is_none() && !osprey_project::needs_assembly(&source.program) {
        return Ok(source.program.clone());
    }
    let root = source
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut config = config
        .cloned()
        .unwrap_or_else(|| ProjectConfig::for_root(root));
    config.entry = Some(source.path.clone());
    osprey_project::assemble(&config, sources)
        .map(|project| project.program)
        .map_err(|errors| {
            errors
                .iter()
                .map(|error| crate::project::format_project_error(error, "doctest"))
                .collect::<Vec<_>>()
                .join("\n")
        })
}

fn inject(statements: &mut Vec<Stmt>, scope: &[usize], body: Program) -> Result<Expr, String> {
    let Some((&index, remaining)) = scope.split_first() else {
        install_snippet(statements, body);
        return Ok(call(Expr::Identifier(runner())));
    };
    let statement = statements
        .get_mut(index)
        .ok_or("documentation scope is missing")?;
    let target = inject_container(statement, remaining, body)?;
    statements.push(function(target));
    Ok(call(Expr::Identifier(runner())))
}

fn inject_container(
    statement: &mut Stmt,
    scope: &[usize],
    example: Program,
) -> Result<Expr, String> {
    match statement {
        Stmt::Namespace { name, body, .. } => {
            let _ = inject(body, scope, example)?;
            Ok(qualified_call(vec![name.label().to_string(), runner()]))
        }
        Stmt::Module { path, body, .. } => {
            inject_module(body, scope, example)?;
            let mut segments = path.segments.clone();
            segments.push(runner());
            Ok(qualified_call(segments))
        }
        _ => Err("documentation scope is not a namespace or module".into()),
    }
}

fn inject_module(
    items: &mut Vec<ModuleItem>,
    scope: &[usize],
    body: Program,
) -> Result<(), String> {
    let mut statements: Vec<_> = items
        .iter()
        .map(|item| (*item.declaration).clone())
        .collect();
    let _ = inject(&mut statements, scope, body)?;
    for (index, declaration) in statements.into_iter().enumerate() {
        if let Some(item) = items.get_mut(index) {
            *item.declaration = declaration;
        } else {
            items.push(ModuleItem {
                visibility: Visibility::Exported,
                opaque: false,
                declaration: Box::new(declaration),
            });
        }
    }
    Ok(())
}

fn discard_entry(statements: &mut Vec<Stmt>) {
    statements.retain(|statement| {
        !matches!(statement, Stmt::Expr { .. } | Stmt::Assignment { .. })
            && !matches!(statement, Stmt::Function { name, .. } if name == "main")
    });
    for statement in statements {
        if let Stmt::Namespace { body, .. } = statement {
            discard_entry(body);
        }
    }
}

fn install_snippet(statements: &mut Vec<Stmt>, snippet: Program) {
    let mut executable = Vec::new();
    let mut main = None;
    for statement in snippet.statements {
        match statement {
            Stmt::Function { name, body, .. } if name == "main" => main = Some(body),
            Stmt::Expr { .. } | Stmt::Let { .. } | Stmt::Assignment { .. } => {
                executable.push(statement);
            }
            declaration => statements.push(declaration),
        }
    }
    statements.push(function(Expr::Block {
        statements: executable,
        value: main.map(Box::new),
    }));
}

fn runner() -> String {
    osprey_ast::generated_name(RUNNER_ROLE, 0)
}

fn qualified_call(segments: Vec<String>) -> Expr {
    call(Expr::Path(SymbolPath::new(segments)))
}

fn call(function: Expr) -> Expr {
    Expr::Call {
        function: Box::new(function),
        arguments: Vec::new(),
        named_arguments: Vec::new(),
    }
}

fn expression(value: Expr) -> Stmt {
    Stmt::Expr {
        value,
        doc: None,
        position: None,
    }
}

fn function(body: Expr) -> Stmt {
    Stmt::Function {
        name: runner(),
        type_params: Vec::new(),
        parameters: Vec::new(),
        return_type: None,
        effects: Vec::new(),
        body,
        doc: None,
        position: None,
    }
}
