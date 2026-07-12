//! Position rebasing for collision-free multi-file inference metadata.

use osprey_ast::{
    EffectOperation, EffectRef, Expr, FieldAssignment, InterpolatedPart, MatchArm, NamedArgument,
    Parameter, Pattern, Position, Program, SignatureItem, SignatureType, Stmt, TypeExpr,
};

pub(crate) fn offset_program(program: &mut Program, line_offset: u32) {
    for statement in &mut program.statements {
        offset_stmt(statement, line_offset);
    }
}

fn offset_stmt(statement: &mut Stmt, offset: u32) {
    match statement {
        Stmt::Import(import) => shift(&mut import.position, offset),
        Stmt::Namespace { body, position, .. } => {
            shift(position, offset);
            offset_stmts(body, offset);
        }
        Stmt::Let {
            ty,
            value,
            position,
            ..
        } => {
            shift(position, offset);
            optional_type(ty, offset);
            offset_expr(value, offset);
        }
        Stmt::Assignment {
            value, position, ..
        }
        | Stmt::Expr {
            value, position, ..
        } => {
            shift(position, offset);
            offset_expr(value, offset);
        }
        Stmt::Function {
            parameters,
            return_type,
            effects,
            body,
            position,
            ..
        } => {
            shift(position, offset);
            offset_params(parameters, offset);
            optional_type(return_type, offset);
            offset_effect_refs(effects, offset);
            offset_expr(body, offset);
        }
        Stmt::Extern {
            parameters,
            return_type,
            position,
            ..
        } => {
            shift(position, offset);
            for parameter in parameters {
                offset_type(&mut parameter.ty, offset);
            }
            optional_type(return_type, offset);
        }
        Stmt::Type {
            variants,
            alias,
            position,
            ..
        } => {
            shift(position, offset);
            optional_type(alias, offset);
            for variant in variants {
                for field in &mut variant.fields {
                    if let Some(constraint) = &mut field.constraint {
                        offset_expr(constraint, offset);
                    }
                }
            }
        }
        Stmt::Effect {
            operations,
            position,
            ..
        } => {
            shift(position, offset);
            offset_operations(operations, offset);
        }
        Stmt::Module { body, position, .. } => {
            shift(position, offset);
            for item in body {
                offset_stmt(&mut item.declaration, offset);
            }
        }
        Stmt::Signature {
            items, position, ..
        } => {
            shift(position, offset);
            for item in items {
                offset_signature_item(item, offset);
            }
        }
    }
}

fn offset_stmts(statements: &mut [Stmt], offset: u32) {
    for statement in statements {
        offset_stmt(statement, offset);
    }
}

fn offset_signature_item(item: &mut SignatureItem, offset: u32) {
    match item {
        SignatureItem::Value { ty, position, .. } => {
            shift(position, offset);
            offset_type(ty, offset);
        }
        SignatureItem::Function {
            parameters,
            return_type,
            effects,
            position,
            ..
        } => {
            shift(position, offset);
            for parameter in parameters {
                offset_type(parameter, offset);
            }
            offset_type(return_type, offset);
            offset_effect_refs(effects, offset);
        }
        SignatureItem::Type {
            definition,
            position,
            ..
        } => {
            shift(position, offset);
            if let SignatureType::Manifest(ty) = definition {
                offset_type(ty, offset);
            }
        }
        SignatureItem::Effect {
            operations,
            position,
            ..
        } => {
            shift(position, offset);
            offset_operations(operations, offset);
        }
        SignatureItem::Module { position, .. } => shift(position, offset),
    }
}

fn offset_operations(operations: &mut [EffectOperation], offset: u32) {
    for operation in operations {
        offset_params(&mut operation.parameters, offset);
    }
}

fn offset_params(parameters: &mut [Parameter], offset: u32) {
    for parameter in parameters {
        optional_type(&mut parameter.ty, offset);
    }
}

fn offset_effect_refs(effects: &mut [EffectRef], offset: u32) {
    for effect in effects {
        shift(&mut effect.position, offset);
        for argument in &mut effect.type_args {
            offset_type(argument, offset);
        }
    }
}

fn optional_type(ty: &mut Option<TypeExpr>, offset: u32) {
    if let Some(ty) = ty {
        offset_type(ty, offset);
    }
}

fn offset_type(ty: &mut TypeExpr, offset: u32) {
    shift(&mut ty.position, offset);
    for parameter in &mut ty.generic_params {
        offset_type(parameter, offset);
    }
    if let Some(element) = &mut ty.array_element {
        offset_type(element, offset);
    }
    for parameter in &mut ty.parameter_types {
        offset_type(parameter, offset);
    }
    if let Some(result) = &mut ty.return_type {
        offset_type(result, offset);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "position rebasing exhaustively visits every canonical expression form"
)]
fn offset_expr(expr: &mut Expr, offset: u32) {
    match expr {
        Expr::InterpolatedStr(parts) => {
            for part in parts {
                if let InterpolatedPart::Expr(value) = part {
                    offset_expr(value, offset);
                }
            }
        }
        Expr::List(items) => offset_exprs(items, offset),
        Expr::Map(entries) => {
            for entry in entries {
                offset_expr(&mut entry.key, offset);
                offset_expr(&mut entry.value, offset);
            }
        }
        Expr::Object(fields) | Expr::Update { fields, .. } => offset_fields(fields, offset),
        Expr::TypeConstructor {
            type_args, fields, ..
        } => {
            for argument in type_args {
                offset_type(argument, offset);
            }
            offset_fields(fields, offset);
        }
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            offset_expr(left, offset);
            offset_expr(right, offset);
        }
        Expr::Unary { operand, .. }
        | Expr::Spawn(operand)
        | Expr::Await(operand)
        | Expr::Recv(operand) => offset_expr(operand, offset),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            offset_expr(function, offset);
            offset_exprs(arguments, offset);
            offset_named(named_arguments, offset);
        }
        Expr::FieldAccess { target, .. } => offset_expr(target, offset),
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            offset_expr(target, offset);
            offset_exprs(arguments, offset);
            offset_named(named_arguments, offset);
        }
        Expr::Index { target, index } => {
            offset_expr(target, offset);
            offset_expr(index, offset);
        }
        Expr::Lambda {
            parameters,
            return_type,
            body,
            position,
        } => {
            shift(position, offset);
            offset_params(parameters, offset);
            optional_type(return_type, offset);
            offset_expr(body, offset);
        }
        Expr::Match { value, arms } => {
            offset_expr(value, offset);
            offset_arms(arms, offset);
        }
        Expr::Block { statements, value } => {
            offset_stmts(statements, offset);
            if let Some(value) = value {
                offset_expr(value, offset);
            }
        }
        Expr::Yield(value) | Expr::Resume(value) => {
            if let Some(value) = value {
                offset_expr(value, offset);
            }
        }
        Expr::Send { channel, value } => {
            offset_expr(channel, offset);
            offset_expr(value, offset);
        }
        Expr::Select { arms } => offset_arms(arms, offset),
        Expr::Perform {
            arguments,
            named_arguments,
            position,
            ..
        } => {
            shift(position, offset);
            offset_exprs(arguments, offset);
            offset_named(named_arguments, offset);
        }
        Expr::Handler {
            arms,
            body,
            position,
            ..
        } => {
            shift(position, offset);
            for arm in arms {
                offset_expr(&mut arm.body, offset);
            }
            offset_expr(body, offset);
        }
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Identifier(_)
        | Expr::Path(_) => {}
    }
}

fn offset_exprs(expressions: &mut [Expr], offset: u32) {
    for expression in expressions {
        offset_expr(expression, offset);
    }
}

fn offset_fields(fields: &mut [FieldAssignment], offset: u32) {
    for field in fields {
        offset_expr(&mut field.value, offset);
    }
}

fn offset_named(arguments: &mut [NamedArgument], offset: u32) {
    for argument in arguments {
        offset_expr(&mut argument.value, offset);
    }
}

fn offset_arms(arms: &mut [MatchArm], offset: u32) {
    for arm in arms {
        offset_pattern(&mut arm.pattern, offset);
        offset_expr(&mut arm.body, offset);
    }
}

fn offset_pattern(pattern: &mut Pattern, offset: u32) {
    match pattern {
        Pattern::Literal(value) => offset_expr(value, offset),
        Pattern::Constructor { sub_patterns, .. } => {
            for sub_pattern in sub_patterns {
                offset_pattern(sub_pattern, offset);
            }
        }
        Pattern::TypeAnnotated { ty, .. } => offset_type(ty, offset),
        Pattern::List { elements, .. } => {
            for element in elements {
                offset_pattern(element, offset);
            }
        }
        Pattern::Wildcard | Pattern::Structural { .. } | Pattern::Binding(_) => {}
    }
}

fn shift(position: &mut Option<Position>, offset: u32) {
    if let Some(position) = position {
        position.line = position.line.saturating_add(offset);
    }
}
