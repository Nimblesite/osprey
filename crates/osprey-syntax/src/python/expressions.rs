//! Python expressions lower directly to canonical expression nodes.
use super::context::{apply, children, position, Lowerer, Result};
use crate::SyntaxError;
use osprey_ast::{Expr, FieldAssignment, MapEntry, NamedArgument, Parameter};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn expr(&mut self, node: Node<'_>) -> Result<Expr> {
        match node.kind() {
            "parenthesized_expression" => self.expr(Self::first(node)?),
            "binary_operator" | "boolean_operator" | "comparison_operator" => self.binary(node),
            "unary_operator" | "not_operator" => self.unary(node),
            "call" => self.call(node),
            "lambda" => self.lambda(node),
            "conditional_expression" => self.conditional(node),
            "list" => Ok(Expr::List(self.exprs(node)?, Some(position(node)))),
            "list_comprehension" => self.list_comprehension(node),
            "set_comprehension" | "dictionary_comprehension" | "generator_expression" => {
                Err(Self::unsupported_comprehension(node))
            }
            "dictionary" => self.map(node),
            "attribute" => self.attribute(node),
            "subscript" => self.index(node),
            _ => self.atom(node),
        }
    }

    /// The diagnostic for a comprehension form that has no Osprey lowering,
    /// naming the explicit construction to write instead.
    fn unsupported_comprehension(node: Node<'_>) -> SyntaxError {
        let advice = match node.kind() {
            "set_comprehension" => {
                "set comprehensions have no Osprey lowering; build the set explicitly"
            }
            "dictionary_comprehension" => {
                "dictionary comprehensions have no Osprey lowering; build a Map with mapSet"
            }
            _ => "generator expressions have no Osprey lowering; write toList(map(...)) or fold",
        };
        Self::error(node, advice)
    }

    fn atom(&mut self, node: Node<'_>) -> Result<Expr> {
        match node.kind() {
            "identifier" => Ok(Expr::Identifier(self.text(node).into())),
            "integer" => self.integer(node),
            "float" => self.float(node),
            "true" | "false" => Ok(Expr::Bool(node.kind() == "true")),
            "none" => Ok(super::context::unit()),
            "string" => self.string(node),
            _ => self.unsupported(node),
        }
    }

    pub(super) fn first(node: Node<'_>) -> Result<Node<'_>> {
        children(node)
            .into_iter()
            .next()
            .ok_or_else(|| Self::error(node, "expected a Python expression"))
    }

    pub(super) fn exprs(&mut self, node: Node<'_>) -> Result<Vec<Expr>> {
        children(node)
            .into_iter()
            .map(|child| self.expr(child))
            .collect()
    }

    fn integer(&self, node: Node<'_>) -> Result<Expr> {
        self.text(node)
            .replace('_', "")
            .parse::<i64>()
            .map(Expr::Integer)
            .map_err(|error| {
                Self::error(
                    node,
                    format!("integer literal is outside the signed 64-bit range: {error}"),
                )
            })
    }

    fn float(&self, node: Node<'_>) -> Result<Expr> {
        match self.text(node).replace('_', "").parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(Expr::Float(value)),
            _ => Err(Self::error(
                node,
                "float literal is outside the finite 64-bit range",
            )),
        }
    }

    fn binary(&mut self, node: Node<'_>) -> Result<Expr> {
        let parts = children(node);
        let [left, right] = parts.as_slice() else {
            return self.unsupported(node);
        };
        let raw = self.operator(node, *left, *right);
        if raw == "//" {
            return self.int_div(*left, *right);
        }
        let op = self.binary_operator(node, &raw)?;
        Ok(Expr::Binary {
            op: op.into(),
            left: Box::new(self.expr(*left)?),
            right: Box::new(self.expr(*right)?),
            position: Some(position(node)),
        })
    }

    fn binary_operator<'a>(&self, node: Node<'_>, raw: &'a str) -> Result<&'a str> {
        match raw.trim() {
            "and" => Ok("&&"),
            "or" => Ok("||"),
            op @ ("+" | "-" | "*" | "/" | "%" | "<" | ">" | "<=" | ">=" | "==" | "!=" | "&&"
            | "||") => Ok(op),
            _ => self.unsupported(node),
        }
    }

    fn operator(&self, node: Node<'_>, left: Node<'_>, right: Node<'_>) -> String {
        node.child_by_field_name("operator").map_or_else(
            || {
                self.text(node)
                    .get(
                        left.end_byte().saturating_sub(node.start_byte())
                            ..right.start_byte().saturating_sub(node.start_byte()),
                    )
                    .unwrap_or_default()
                    .trim()
                    .into()
            },
            |op| self.text(op).into(),
        )
    }

    fn int_div(&mut self, left: Node<'_>, right: Node<'_>) -> Result<Expr> {
        Ok(invoke("intDiv", vec![self.expr(left)?, self.expr(right)?]))
    }

    fn unary(&mut self, node: Node<'_>) -> Result<Expr> {
        let operand = Self::field(node, "argument").or_else(|_| Self::first(node))?;
        let negative = self.text(node).trim_start().starts_with('-');
        if !negative && node.kind() != "not_operator" {
            return self.unsupported(node);
        }
        if negative && self.text(operand).replace('_', "") == crate::I64_MIN_MAGNITUDE {
            return Ok(Expr::Integer(i64::MIN));
        }
        let value = self.expr(operand)?;
        Ok(if negative {
            Expr::negated(value)
        } else {
            Expr::Unary {
                op: "!".into(),
                operand: Box::new(value),
            }
        })
    }

    fn call(&mut self, node: Node<'_>) -> Result<Expr> {
        let function = Self::field(node, "function")?;
        if let Some(value) = self.intrinsic(node, function)? {
            return Ok(value);
        }
        let (mut arguments, named_arguments) = self.arguments(Self::field(node, "arguments")?)?;
        let raw = self.text(function);
        if raw == "range" && self.lookup(raw).is_none() && arguments.len() == 1 {
            arguments.insert(0, Expr::Integer(0));
        }
        let callee = self.call_alias(function)?;
        self.construct_or_call(node, callee, arguments, named_arguments)
    }

    fn call_alias(&mut self, node: Node<'_>) -> Result<Expr> {
        if self.text(node) == "len" && self.lookup("len").is_none() {
            Ok(Expr::Identifier("length".into()))
        } else {
            self.call_target(node)
        }
    }

    fn call_target(&mut self, node: Node<'_>) -> Result<Expr> {
        if node.kind() != "subscript" {
            return self.expr(node);
        }
        let name = Self::field(node, "value")?;
        let type_args = self.type_arguments(node, 1)?;
        Ok(Expr::TypeApply {
            function: Box::new(self.expr(name)?),
            type_args,
            position: Some(position(name)),
        })
    }

    fn construct_or_call(
        &self,
        node: Node<'_>,
        callee: Expr,
        arguments: Vec<Expr>,
        named: Vec<NamedArgument>,
    ) -> Result<Expr> {
        // A positional payload has no field names to supply: `JsonString("x")`
        // is a saturated application of a positionally-declared constructor and
        // stays a call so assembly folds it against the declaring unit (the
        // built-in `Json` prelude) ([TYPE-UNION-POSITIONAL]). A call with no
        // positional argument — named fields or none — is a construction.
        let head = self.constructor_head(&callee);
        let Some((name, type_args)) = head.filter(|_| arguments.is_empty()) else {
            return Ok(Expr::Call {
                function: Box::new(callee),
                arguments,
                named_arguments: named,
            });
        };
        Self::named_fields_only(node, &arguments)?;
        Ok(construct(name, type_args, named))
    }

    fn named_fields_only(node: Node<'_>, arguments: &[Expr]) -> Result<()> {
        if !arguments.is_empty() {
            return Err(Self::error(
                node,
                "record construction requires named fields",
            ));
        }
        Ok(())
    }

    /// The type a call constructs, if it constructs one. A name declared as a
    /// class here always constructs. Imported names remain calls until the
    /// project resolver distinguishes their declarations: capitalization alone
    /// cannot distinguish a nullary factory from a record constructor.
    fn constructor_head(&self, callee: &Expr) -> Option<(String, Vec<osprey_ast::TypeExpr>)> {
        let (head, type_args) = match callee {
            Expr::TypeApply {
                function,
                type_args,
                ..
            } => (function.as_ref(), type_args.clone()),
            other => (other, Vec::new()),
        };
        match head {
            Expr::Identifier(name) if self.is_constructor(name) => Some((name.clone(), type_args)),
            _ => None,
        }
    }

    pub(super) fn arguments(&mut self, node: Node<'_>) -> Result<(Vec<Expr>, Vec<NamedArgument>)> {
        let mut positional = Vec::new();
        let mut named = Vec::new();
        for child in children(node) {
            if child.kind() == "keyword_argument" {
                named.push(NamedArgument {
                    name: self.text(Self::field(child, "name")?).into(),
                    value: self.expr(Self::field(child, "value")?)?,
                });
            } else {
                positional.push(self.expr(child)?);
            }
        }
        Ok((positional, named))
    }

    fn lambda(&mut self, node: Node<'_>) -> Result<Expr> {
        self.scoped(true, |lowerer| {
            let parameters = lowerer.parameters(node)?;
            let body = Box::new(lowerer.expr(Lowerer::field(node, "body")?)?);
            Ok(Expr::Lambda {
                parameters,
                return_type: None,
                body,
                position: Some(position(node)),
            })
        })
    }

    pub(super) fn parameters(&mut self, node: Node<'_>) -> Result<Vec<Parameter>> {
        let Some(parameters) = node.child_by_field_name("parameters") else {
            return Ok(Vec::new());
        };
        children(parameters)
            .into_iter()
            .map(|parameter| self.parameter(parameter, node))
            .collect()
    }

    fn conditional(&mut self, node: Node<'_>) -> Result<Expr> {
        let parts = children(node);
        let [then, condition, otherwise] = parts.as_slice() else {
            return self.unsupported(node);
        };
        Ok(crate::desugar::bool_match(
            self.expr(*condition)?,
            self.expr(*then)?,
            self.expr(*otherwise)?,
        ))
    }

    fn map(&mut self, node: Node<'_>) -> Result<Expr> {
        let entries = children(node)
            .into_iter()
            .map(|entry| {
                if entry.kind() != "pair" {
                    return self.unsupported(entry);
                }
                Ok(MapEntry {
                    key: self.expr(Self::field(entry, "key")?)?,
                    value: self.expr(Self::field(entry, "value")?)?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Expr::Map(entries))
    }

    fn attribute(&mut self, node: Node<'_>) -> Result<Expr> {
        let target = self.expr(Self::field(node, "object")?)?;
        let field = self.text(Self::field(node, "attribute")?).to_owned();
        let segments = match &target {
            Expr::Path(path) => Some(path.segments.clone()),
            Expr::Identifier(name) if self.imported_name(name) => Some(vec![name.clone()]),
            _ => None,
        };
        if let Some(mut segments) = segments {
            segments.push(field);
            return Ok(Expr::Path(osprey_ast::SymbolPath::new(segments)));
        }
        Ok(Expr::FieldAccess {
            target: Box::new(target),
            field,
        })
    }

    fn index(&mut self, node: Node<'_>) -> Result<Expr> {
        Ok(Expr::Index {
            target: Box::new(self.expr(Self::field(node, "value")?)?),
            index: Box::new(self.expr(Self::field(node, "subscript")?)?),
        })
    }
}

fn construct(
    name: String,
    type_args: Vec<osprey_ast::TypeExpr>,
    named: Vec<NamedArgument>,
) -> Expr {
    Expr::TypeConstructor {
        name,
        type_args,
        fields: fields(named),
    }
}

pub(super) fn fields(named: Vec<NamedArgument>) -> Vec<FieldAssignment> {
    named
        .into_iter()
        .map(|arg| FieldAssignment {
            name: arg.name,
            value: arg.value,
        })
        .collect()
}

pub(super) fn invoke(name: &str, arguments: Vec<Expr>) -> Expr {
    apply(Expr::Identifier(name.into()), arguments)
}
