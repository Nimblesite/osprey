//! An object literal takes the record type its context declares, in every
//! flavor: `let p: Point = { x: 1, y: 2 }` constructs a `Point`. The context
//! is a declared type, never an inferred one: an annotated binding, a declared
//! return type, an annotated parameter of a called function, a declared record
//! field, and the elements of a list of records. The rewrite runs before
//! checking and code generation, so both see the same `Point` constructor and
//! the record's declared layout. A literal whose fields differ from the
//! record's stays anonymous, and the checker reports the mismatch.
//! Implements [TYPE-RECORD-CONTEXT] and [FLAVOR-TS-UNIONS].
use crate::mutate::{children_mut, statement_children_mut};
use crate::{Expr, Program, Stmt, TypeExpr};
use std::collections::{BTreeMap, BTreeSet};

/// The record type a context declares for an expression.
#[derive(Clone)]
enum Expected {
    Record(String, Vec<TypeExpr>),
    List(Box<Expected>),
}

/// The declarations a context is read from.
#[derive(Default)]
struct Declarations {
    /// Each record type's declared fields, by field name.
    records: BTreeMap<String, BTreeMap<String, String>>,
    /// Each function's declared parameter types; `None` when two functions
    /// share the name, so no call is read as either.
    parameters: BTreeMap<String, Option<Vec<Option<TypeExpr>>>>,
}

/// `program` with every object literal in a declared record context rewritten
/// into that record's constructor.
#[must_use]
pub fn elaborate(program: &Program) -> Program {
    let mut declarations = Declarations::default();
    program
        .statements
        .iter()
        .for_each(|statement| declarations.collect(statement));
    let mut elaborated = program.clone();
    for statement in &mut elaborated.statements {
        declarations.statement(statement);
    }
    elaborated
}

impl Declarations {
    fn collect(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Namespace { body, .. } => {
                for nested in body {
                    self.collect(nested);
                }
            }
            Stmt::Module { body, .. } => {
                for item in body {
                    self.collect(&item.declaration);
                }
            }
            Stmt::Type {
                name,
                variants,
                alias: None,
                ..
            } => {
                if let [variant] = variants.as_slice() {
                    if variant.name == *name {
                        let fields = variant
                            .fields
                            .iter()
                            .map(|f| (f.name.clone(), f.ty.clone()))
                            .collect();
                        let _ = self.records.insert(name.clone(), fields);
                    }
                }
            }
            Stmt::Function {
                name, parameters, ..
            } => {
                let types = parameters
                    .iter()
                    .map(|parameter| parameter.ty.clone())
                    .collect();
                let entry = self.parameters.entry(name.clone()).or_insert(Some(types));
                if entry
                    .as_ref()
                    .is_some_and(|known| known.len() != parameters.len())
                {
                    *entry = None;
                }
            }
            _ => {}
        }
    }

    fn statement(&self, statement: &mut Stmt) {
        match statement {
            Stmt::Namespace { body, .. } => {
                for nested in body {
                    self.statement(nested);
                }
            }
            Stmt::Module { body, .. } => {
                for item in body {
                    self.statement(&mut item.declaration);
                }
            }
            Stmt::Let {
                ty: Some(ty),
                value,
                ..
            } => {
                let expected = self.expected(ty);
                self.expr(value, expected.as_ref());
            }
            Stmt::Function {
                return_type: Some(ty),
                body,
                ..
            } => {
                let expected = self.expected(ty);
                self.expr(body, expected.as_ref());
            }
            other => statement_children_mut(other, &mut |child| self.expr(child, None)),
        }
    }

    fn expr(&self, expression: &mut Expr, expected: Option<&Expected>) {
        match (expression, expected) {
            (literal @ Expr::Object(_), Some(Expected::Record(name, arguments))) => {
                if let Some(fields) = self.matching(literal, name) {
                    *literal = Expr::TypeConstructor {
                        name: name.clone(),
                        type_args: arguments.clone(),
                        fields,
                    };
                }
                self.expr_children(literal);
            }
            (Expr::List(values, _), Some(Expected::List(element))) => {
                for value in values.iter_mut() {
                    self.expr(value, Some(element));
                }
            }
            (
                Expr::Block {
                    statements, value, ..
                },
                expected,
            ) => {
                for statement in statements.iter_mut() {
                    self.statement(statement);
                }
                if let Some(value) = value {
                    self.expr(value, expected);
                }
            }
            (Expr::Match { value, arms }, expected) => {
                self.expr(value, None);
                for arm in arms.iter_mut() {
                    self.expr(&mut arm.body, expected);
                }
            }
            (
                Expr::Lambda {
                    return_type: Some(ty),
                    body,
                    ..
                },
                _,
            ) => {
                let expected = self.expected(ty);
                self.expr(body, expected.as_ref());
            }
            (other, _) => self.expr_children(other),
        }
    }

    /// Recurse with the contexts a node itself declares: a record
    /// constructor's field types and a call's parameter types.
    fn expr_children(&self, expression: &mut Expr) {
        match expression {
            Expr::TypeConstructor { name, fields, .. } => {
                let declared = self.records.get(name.as_str());
                for field in fields {
                    let expected = declared
                        .and_then(|declared| declared.get(&field.name))
                        .and_then(|ty| self.expected_text(ty));
                    self.expr(&mut field.value, expected.as_ref());
                }
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                let declared = match function.as_ref() {
                    Expr::Identifier(name) => self.parameters.get(name).cloned().flatten(),
                    _ => None,
                };
                self.expr(function, None);
                for (index, argument) in arguments.iter_mut().enumerate() {
                    let expected = declared
                        .as_ref()
                        .and_then(|types| types.get(index).cloned().flatten())
                        .and_then(|ty| self.expected(&ty));
                    self.expr(argument, expected.as_ref());
                }
                for named in named_arguments.iter_mut() {
                    self.expr(&mut named.value, None);
                }
            }
            other => children_mut(other, &mut |child| self.expr(child, None)),
        }
    }

    /// The literal's fields, when they are exactly the record's declared fields.
    fn matching(&self, literal: &Expr, name: &str) -> Option<Vec<crate::FieldAssignment>> {
        let Expr::Object(fields) = literal else {
            return None;
        };
        let declared: BTreeSet<&String> = self.records.get(name)?.keys().collect();
        let written: BTreeSet<&String> = fields.iter().map(|field| &field.name).collect();
        (declared == written && written.len() == fields.len()).then(|| fields.clone())
    }

    fn expected(&self, ty: &TypeExpr) -> Option<Expected> {
        if ty.is_array {
            let element = ty.array_element.as_deref()?;
            return self
                .expected(element)
                .map(|inner| Expected::List(Box::new(inner)));
        }
        match (ty.name.as_str(), ty.generic_params.as_slice()) {
            ("List", [element]) => self
                .expected(element)
                .map(|inner| Expected::List(Box::new(inner))),
            (name, arguments) if self.records.contains_key(name) => {
                Some(Expected::Record(name.to_owned(), arguments.to_vec()))
            }
            _ => None,
        }
    }

    /// The context a declared field type written as text gives: a record named
    /// alone, or a list of one.
    fn expected_text(&self, ty: &str) -> Option<Expected> {
        let ty = ty.trim();
        if let Some(element) = ty
            .strip_prefix("List<")
            .and_then(|rest| rest.strip_suffix('>'))
        {
            return self
                .expected_text(element)
                .map(|inner| Expected::List(Box::new(inner)));
        }
        self.records
            .contains_key(ty)
            .then(|| Expected::Record(ty.to_owned(), Vec::new()))
    }
}
