//! TypeScript statements lower to Osprey blocks and flat functions.
//! Implements [FLAVOR-TS-FUNCTIONS], [FLAVOR-TS-RECEIVER], [FLAVOR-TS-UNIONS].
use super::context::{apply, unit, Expected, Lowerer, Narrowing, Result};
use super::types::{variant_name, KIND};
use super::updates::{assigned_root, discard_unread, receiver_roots, statement_root};
use osprey_ast::{Expr, MatchArm, Parameter, Pattern, Stmt};
use oxc_ast::ast::{
    BindingPattern, Expression, ForOfStatement, ForStatementLeft, FormalParameters, Function,
    IfStatement, Statement, SwitchCase, SwitchStatement, VariableDeclaration,
    VariableDeclarationKind,
};
use oxc_span::{GetSpan, Span};

/// A function body: a statement list, or an arrow's single expression.
pub(super) enum Body<'b, 'a> {
    Statements(&'b [Statement<'a>], Span),
    Expression(&'b Expression<'a>),
}

fn ends_in_return(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::ReturnStatement(_) => true,
        Statement::BlockStatement(block) => block.body.last().is_some_and(ends_in_return),
        _ => false,
    }
}

impl Lowerer<'_> {
    pub(super) fn function(&mut self, node: &Function<'_>) -> Result<Stmt> {
        if !self.file_scope() {
            return Err(self.error(
                node.span,
                "a nested `function` has no Osprey lowering; bind an arrow function instead",
            ));
        }
        if node.r#async || node.generator {
            return self.unsupported(node.span, "`async` or generator function; use fibers");
        }
        let (Some(id), Some(body)) = (&node.id, &node.body) else {
            return self.unsupported(node.span, "function without a name or a body");
        };
        let name = id.name.as_str().to_owned();
        self.bind(&name);
        let position = Some(self.at(node.span));
        self.scoped(true, true, |lowerer| {
            let parameters = lowerer.parameters(&node.params)?;
            let returns = lowerer.expected(node.return_type.as_deref());
            let statements = Body::Statements(&body.statements, body.span);
            let (return_type, effects) = match node.return_type.as_deref() {
                Some(annotation) => {
                    let (result, effects) = lowerer.requires(&annotation.type_annotation)?;
                    (Some(lowerer.type_expr(result)?), effects)
                }
                None => (None, None),
            };
            Ok(Stmt::Function {
                name,
                type_params: lowerer.type_parameters(node.type_parameters.as_deref())?,
                return_type,
                body: lowerer.function_body(&node.params, returns, &statements)?,
                parameters,
                effect_row_present: effects.is_some(),
                effects: effects.unwrap_or_default(),
                effect_tail: None,
                doc: None,
                position,
            })
        })
    }

    /// Lower a body in the function scope its parameters are already bound in.
    pub(super) fn function_body(
        &mut self,
        parameters: &FormalParameters<'_>,
        returns: Option<Expected>,
        body: &Body<'_, '_>,
    ) -> Result<Expr> {
        match body {
            Body::Expression(expression) => {
                self.set_function_result(None, returns.clone());
                self.expr_as(expression, returns.as_ref())
            }
            Body::Statements(statements, span) => {
                let updated = self.updated_parameter_of(parameters, statements, *span)?;
                self.set_function_result(updated, returns);
                self.block(&statements.iter().collect::<Vec<_>>(), *span)
            }
        }
    }

    /// The one parameter a body updates through receiver statements: the
    /// function returns its final value. Implements [FLAVOR-TS-RECEIVER].
    fn updated_parameter_of(
        &self,
        parameters: &FormalParameters<'_>,
        statements: &[Statement<'_>],
        span: Span,
    ) -> Result<Option<String>> {
        let mut roots = Vec::new();
        receiver_roots(statements, &mut roots);
        let mut updated: Vec<&str> = parameters
            .items
            .iter()
            .filter_map(|parameter| match &parameter.pattern {
                BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
                _ => None,
            })
            .filter(|name| roots.contains(name))
            .collect();
        updated.dedup();
        match updated.as_slice() {
            [] => Ok(None),
            [name] => Ok(Some((*name).to_owned())),
            _ => Err(self.error(
                span,
                "a function can update only one parameter in place; return the others",
            )),
        }
    }

    pub(super) fn binding_name(&self, pattern: &BindingPattern<'_>) -> Result<String> {
        match pattern {
            BindingPattern::BindingIdentifier(id) => Ok(id.name.as_str().to_owned()),
            other => self.unsupported(other.span(), "destructuring"),
        }
    }

    /// Lower a statement list in the current scope; a tail block yields the
    /// value its last statement returns.
    pub(super) fn block(&mut self, statements: &[&Statement<'_>], span: Span) -> Result<Expr> {
        let tail = self.in_tail();
        let mut lowered = Vec::new();
        let mut rebound = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            let rest = statements.get(index + 1..).unwrap_or_default();
            if let Some(value) = self.tail_value(statement, rest, tail)? {
                return Ok(self.finish(lowered, Some(value), statement.span(), &rebound));
            }
            if statement_root(statement)
                .or_else(|| assigned_root(statement))
                .is_some()
            {
                rebound.push(lowered.len());
            }
            lowered.extend(self.statement(statement)?);
        }
        let value = self.implicit_result(tail);
        Ok(self.finish(lowered, value, span, &rebound))
    }

    fn finish(
        &mut self,
        mut statements: Vec<Stmt>,
        value: Option<Expr>,
        span: Span,
        rebound: &[usize],
    ) -> Expr {
        discard_unread(&mut statements, value.as_ref(), rebound);
        let state = self.take_mutated();
        for statement in &mut statements {
            if let Stmt::Let { name, mutable, .. } = statement {
                *mutable |= state.contains(name.as_str());
            }
        }
        let position = Some(self.at(span));
        let value = value.map(|value| Box::new(crate::desugar::positioned(value, position)));
        crate::desugar::block(statements, value, position)
    }

    /// What a tail block yields when it ends without `return`.
    fn implicit_result(&self, tail: bool) -> Option<Expr> {
        self.updated_parameter()
            .filter(|_| tail)
            .map(Expr::Identifier)
    }

    fn tail_value(
        &mut self,
        statement: &Statement<'_>,
        rest: &[&Statement<'_>],
        tail: bool,
    ) -> Result<Option<Expr>> {
        if !tail {
            return Ok(None);
        }
        match statement {
            Statement::ReturnStatement(returned) => {
                if let Some(next) = rest.first() {
                    return Err(self.error(next.span(), "this statement follows a `return`"));
                }
                let expected = self.expected_return();
                let value = match &returned.argument {
                    Some(argument) => self.expr_as(argument, expected.as_ref())?,
                    None => self.implicit_result(true).unwrap_or_else(unit),
                };
                Ok(Some(value))
            }
            Statement::IfStatement(branch) if rest.is_empty() => {
                self.conditional(branch, None, true).map(Some)
            }
            Statement::IfStatement(branch)
                if branch.alternate.is_none() && ends_in_return(&branch.consequent) =>
            {
                self.conditional(branch, Some(rest), true).map(Some)
            }
            Statement::SwitchStatement(switch) if rest.is_empty() => {
                self.switch(switch, true).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// `if` as the shared boolean match. A guard clause (`if (c) { …; return }`)
    /// takes the statements after it as its `else`.
    fn conditional(
        &mut self,
        branch: &IfStatement<'_>,
        rest: Option<&[&Statement<'_>]>,
        tail: bool,
    ) -> Result<Expr> {
        let condition = self.expr(&branch.test)?;
        let then = self.branch(&branch.consequent, tail)?;
        let otherwise = match (&branch.alternate, rest) {
            (Some(alternate), _) => self.branch(alternate, tail)?,
            (None, Some(rest)) => {
                self.scoped(false, tail, |lowerer| lowerer.block(rest, branch.span))?
            }
            (None, None) => self.implicit_result(tail).unwrap_or_else(unit),
        };
        Ok(crate::desugar::bool_match(condition, then, otherwise))
    }

    fn branch(&mut self, statement: &Statement<'_>, tail: bool) -> Result<Expr> {
        self.scoped(false, tail, |lowerer| lowerer.statements_of(statement))
    }

    /// The block a braced body is, or the one statement written without braces.
    fn statements_of(&mut self, statement: &Statement<'_>) -> Result<Expr> {
        match statement {
            Statement::BlockStatement(block) => {
                self.block(&block.body.iter().collect::<Vec<_>>(), block.span)
            }
            other => self.block(&[other], other.span()),
        }
    }

    pub(super) fn statement(&mut self, statement: &Statement<'_>) -> Result<Vec<Stmt>> {
        let value = match statement {
            Statement::VariableDeclaration(declaration) => return self.variables(declaration),
            Statement::ExpressionStatement(expression) => {
                return self
                    .expression_statement(expression)
                    .map(|lowered| vec![lowered]);
            }
            Statement::EmptyStatement(_) => return Ok(Vec::new()),
            Statement::IfStatement(branch) => self.conditional(branch, None, false)?,
            Statement::SwitchStatement(switch) => self.switch(switch, false)?,
            Statement::ForOfStatement(each) => self.for_of(each)?,
            Statement::ReturnStatement(returned) => {
                return Err(self.error(
                    returned.span,
                    "`return` is only allowed in tail position; an early return is rejected",
                ));
            }
            Statement::FunctionDeclaration(function) => {
                return self.function(function).map(|f| vec![f])
            }
            other => return self.unsupported(other.span(), rejected(other)),
        };
        Ok(vec![Stmt::Expr {
            value,
            doc: None,
            position: Some(self.at(statement.span())),
        }])
    }

    pub(super) fn variables(&mut self, declaration: &VariableDeclaration<'_>) -> Result<Vec<Stmt>> {
        if declaration.declare
            || !matches!(
                declaration.kind,
                VariableDeclarationKind::Const | VariableDeclarationKind::Let
            )
        {
            return self.unsupported(declaration.span, "`var`, `using` or `declare` binding");
        }
        declaration
            .declarations
            .iter()
            .map(|declarator| {
                let name = self.binding_name(&declarator.id)?;
                let Some(init) = &declarator.init else {
                    return self.unsupported(declarator.span, "binding without a value");
                };
                let expected = self.expected(declarator.type_annotation.as_deref());
                let value = self.expr_as(init, expected.as_ref())?;
                let ty = self.annotation(declarator.type_annotation.as_deref())?;
                if declaration.kind == VariableDeclarationKind::Const {
                    self.bind_constant(&name);
                } else {
                    self.bind(&name);
                }
                Ok(self.let_statement(&name, value, ty, declarator.span))
            })
            .collect()
    }

    /// `for (const x of xs) { … }` is `forEach(xs, x => { … })`.
    fn for_of(&mut self, each: &ForOfStatement<'_>) -> Result<Expr> {
        if each.r#await {
            return self.unsupported(each.span, "`for await`; use fibers");
        }
        let ForStatementLeft::VariableDeclaration(declaration) = &each.left else {
            return self.unsupported(each.span, "`for … of` over an existing binding");
        };
        let [declarator] = declaration.declarations.as_slice() else {
            return self.unsupported(declaration.span, "`for … of` with several bindings");
        };
        let name = self.binding_name(&declarator.id)?;
        let iterator = self.expr(&each.right)?;
        let body = self.scoped(true, false, |lowerer| {
            lowerer.bind(&name);
            lowerer.set_function_result(None, None);
            lowerer.statements_of(&each.body)
        })?;
        let callback = Expr::Lambda {
            parameters: vec![Parameter {
                name,
                ty: None,
                inline_constraint: false,
            }],
            return_type: None,
            body: Box::new(body),
            position: Some(self.at(each.span)),
        };
        Ok(apply("forEach", vec![iterator, callback]))
    }

    /// `switch (value.kind)` is an exhaustive match over the union's variants.
    /// Adjacent `case` labels with no statements between them share the next
    /// case's body: one arm whose pattern is the or-pattern of their variants
    /// ([PATTERN-OR-SYNTAX]). A label left with no body after it is the
    /// fallthrough the flavor rejects.
    fn switch(&mut self, switch: &SwitchStatement<'_>, tail: bool) -> Result<Expr> {
        let subject = self.switch_subject(switch)?;
        let mut arms = Vec::new();
        let mut labels: Vec<String> = Vec::new();
        for case in &switch.cases {
            let name = self.case_variant(case)?;
            if case.consequent.is_empty() {
                labels.push(name);
                continue;
            }
            let shared = std::mem::take(&mut labels);
            arms.push(self.case(case, name, shared, &subject, tail)?);
        }
        if let Some(last) = switch.cases.last().filter(|_| !labels.is_empty()) {
            return Err(self.error(
                last.span,
                "a `case` must end with a body; fallthrough is rejected",
            ));
        }
        Ok(Expr::Match {
            value: Box::new(Expr::Identifier(subject)),
            arms,
        })
    }

    /// The variant a `case` label names.
    fn case_variant(&self, case: &SwitchCase<'_>) -> Result<String> {
        let Some(Expression::StringLiteral(kind)) = &case.test else {
            return Err(self.error(
                case.span,
                "each `case` names one `kind` string; `default` is rejected, list every kind",
            ));
        };
        Ok(variant_name(&self.string(kind)?))
    }

    fn switch_subject(&self, switch: &SwitchStatement<'_>) -> Result<String> {
        if let Expression::StaticMemberExpression(member) = &switch.discriminant {
            if let (Expression::Identifier(subject), KIND) =
                (&member.object, member.property.name.as_str())
            {
                return Ok(subject.name.as_str().to_owned());
            }
        }
        Err(self.error(
            switch.discriminant.span(),
            "a `switch` must test `value.kind` of a discriminated union",
        ))
    }

    /// One arm: the variants of the labels before it (`shared`) and its own,
    /// each binding the fields the body reads.
    fn case(
        &mut self,
        case: &SwitchCase<'_>,
        name: String,
        shared: Vec<String>,
        subject: &str,
        tail: bool,
    ) -> Result<MatchArm> {
        let statements = self.case_statements(case, tail)?;
        self.narrowed.push(Narrowing {
            subject: subject.to_owned(),
            span: case.span,
            fields: Vec::new(),
        });
        let body = self.scoped(false, tail, |lowerer| lowerer.block(&statements, case.span));
        let fields = self
            .narrowed
            .pop()
            .map_or_default(|narrowing| narrowing.fields);
        let mut alternatives: Vec<Pattern> = shared
            .into_iter()
            .map(|label| variant_pattern(label, fields.clone()))
            .collect();
        alternatives.push(variant_pattern(name, fields));
        let pattern = if alternatives.len() == 1 {
            alternatives.pop().unwrap_or(Pattern::Wildcard)
        } else {
            Pattern::Or(alternatives)
        };
        Ok(MatchArm {
            pattern,
            body: crate::desugar::positioned(body?, Some(self.at(case.span))),
        })
    }

    /// A case's own statements: no fallthrough, and no `break` left behind.
    fn case_statements<'b, 'a>(
        &self,
        case: &'b SwitchCase<'a>,
        tail: bool,
    ) -> Result<Vec<&'b Statement<'a>>> {
        let mut statements: Vec<_> = match case.consequent.as_slice() {
            [Statement::BlockStatement(block)] => block.body.iter().collect(),
            other => other.iter().collect(),
        };
        let ends = match statements.last() {
            Some(Statement::BreakStatement(_)) if !tail => statements.pop().is_some(),
            Some(last) => tail && ends_in_return(last),
            None => false,
        };
        if !ends {
            let ending = if tail { "`return`" } else { "`break`" };
            return Err(self.error(
                case.span,
                format!("a `case` must end with {ending}; fallthrough is rejected"),
            ));
        }
        Ok(statements)
    }
}

/// A case's pattern binds the fields its body reads; one that reads none is
/// the bare constructor name, as the Default flavor spells it.
/// A case's pattern: each field read binds under its binder, by name, or by
/// role for `Success`/`Error`, as Default's `Success(found)` does.
fn variant_pattern(name: String, fields: Vec<(String, String)>) -> Pattern {
    if fields.is_empty() {
        return Pattern::Binding(name);
    }
    let renamed = fields.iter().any(|(field, binder)| field != binder);
    let (fields, binders): (Vec<_>, Vec<_>) = fields.into_iter().unzip();
    if !renamed {
        return Pattern::Constructor {
            name,
            fields,
            sub_patterns: Vec::new(),
        };
    }
    if crate::desugar::is_result_ctor(&name) {
        return crate::desugar::ctor_pattern(name, binders);
    }
    Pattern::Constructor {
        name,
        fields,
        sub_patterns: binders.into_iter().map(Pattern::Binding).collect(),
    }
}

/// The name a rejected statement form is reported under.
fn rejected(statement: &Statement<'_>) -> &'static str {
    match statement {
        Statement::WhileStatement(_) | Statement::DoWhileStatement(_) => "`while` loop",
        Statement::ForStatement(_) | Statement::ForInStatement(_) => "`for (;;)` or `for … in`",
        Statement::BreakStatement(_) | Statement::ContinueStatement(_) => "`break`/`continue`",
        Statement::ThrowStatement(_) | Statement::TryStatement(_) => {
            "`throw`/`try`; return a `Result`"
        }
        Statement::ClassDeclaration(_) => "`class`",
        Statement::TSEnumDeclaration(_) => "`enum`",
        Statement::LabeledStatement(_) => "label",
        Statement::BlockStatement(_) => "bare block statement",
        _ => "statement form",
    }
}
