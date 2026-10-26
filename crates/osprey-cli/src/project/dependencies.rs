//! Checked source requirements for [PACKAGE-EDITOR-USAGE-GRAPH].
use super::{CompilationInput, CompilationUnit};
use osprey_ast::{AstNode, Position};
use osprey_project::{AssembledProject, SourceDependency};
use std::collections::{BTreeSet, HashMap};

impl CompilationInput {
    pub(crate) fn dependency_graph_json(&self) -> Result<String, String> {
        let CompilationUnit::Project(project) = &self.unit else {
            return Err("source dependency graphs require a project".into());
        };
        let reads = osprey_types::checked_method_reads(&project.program)
            .map_err(|errors| format!("source dependency analysis failed: {errors:?}"))?;
        let mut edges: BTreeSet<_> = project.source_dependencies.iter().cloned().collect();
        for statement in &project.program.statements {
            collect(
                project,
                &reads,
                AstNode::Statement(statement),
                None,
                &mut edges,
            )?;
        }
        Ok(graph_json(project, &edges).to_string())
    }
}

fn collect(
    project: &AssembledProject,
    reads: &HashMap<usize, String>,
    node: AstNode<'_>,
    position: Option<Position>,
    edges: &mut BTreeSet<SourceDependency>,
) -> Result<(), String> {
    let mut pending = vec![(node, position)];
    while let Some((node, position)) = pending.pop() {
        let position = node_position(node, position);
        if let AstNode::Expression(expression) = node {
            if let Some(name) = reads.get(&std::ptr::from_ref(expression).addr()) {
                add_method(project, name, position, edges)?;
            }
        }
        node.for_each_child(|child| pending.push((child, position)));
    }
    Ok(())
}

fn node_position(node: AstNode<'_>, inherited: Option<Position>) -> Option<Position> {
    match node {
        AstNode::Statement(statement) => {
            osprey_ast::stmt_meta::statement_position(statement).or(inherited)
        }
        AstNode::Expression(_) => inherited,
    }
}

fn add_method(
    project: &AssembledProject,
    name: &str,
    position: Option<Position>,
    edges: &mut BTreeSet<SourceDependency>,
) -> Result<(), String> {
    let position = position.ok_or("method call has no source position")?;
    let (source, _) = project
        .source_at_line(position.line)
        .ok_or("method call has no physical source")?;
    let symbol = project
        .source_name_by_mangled
        .get(name)
        .map_or(name, String::as_str);
    edges.extend(
        project
            .method_candidates
            .iter()
            .filter(|edge| edge.source == source.path && edge.symbol == symbol)
            .cloned(),
    );
    Ok(())
}

fn graph_json(project: &AssembledProject, edges: &BTreeSet<SourceDependency>) -> serde_json::Value {
    serde_json::json!({
        "schema": "osprey.source-graph.v1",
        "namespaceImports": "all-providers",
        "sources": project.sources.iter().map(|source| &source.path).collect::<Vec<_>>(),
        "dependencies": edges.iter().map(|edge| serde_json::json!({
            "source": edge.source, "target": edge.target, "symbol": edge.symbol
        })).collect::<Vec<_>>()
    })
}
