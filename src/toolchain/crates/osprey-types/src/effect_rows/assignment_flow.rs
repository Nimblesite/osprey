use super::{
    merge_value, walk_children, Analyzer, BTreeSet, CallableEnv, Expr, HashMap, Stmt, Value,
};

pub(super) type Changes = HashMap<String, Value>;

pub(super) fn merge_changes(target: &mut Changes, changes: Changes) {
    for (name, value) in changes {
        let _ = target
            .entry(name)
            .and_modify(|slot| slot.union(value.clone()))
            .or_insert(value);
    }
}

impl Analyzer<'_> {
    pub(super) fn flow_callable_assignments(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Changes {
        match expression {
            Expr::Block {
                statements, value, ..
            } => self.assignment_block(statements, value.as_deref(), scope, env),
            Expr::Send { channel, value } => self.assignment_send(channel, value, scope, env),
            Expr::Lambda { .. } => Changes::new(),
            _ => self.assignment_children(expression, scope, env),
        }
    }

    fn assignment_children(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Changes {
        let mut changes = Changes::new();
        for child in executed_children(expression) {
            merge_changes(
                &mut changes,
                self.flow_callable_assignments(child, scope, env),
            );
        }
        changes
    }

    pub(super) fn assignment_block(
        &self,
        statements: &[Stmt],
        value: Option<&Expr>,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Changes {
        let mut local = env.clone();
        let (mut writes, names) = self.assignment_declarations(statements, scope, &mut local);
        if let Some(value) = value {
            let changes = self.flow_callable_assignments(value, scope, &mut local);
            merge_changes(&mut writes, visible_changes(changes, &names));
        }
        merge_changes(&mut env.values, writes.clone());
        merge_channels(env, local);
        writes
    }

    fn assignment_declarations(
        &self,
        statements: &[Stmt],
        scope: &[String],
        env: &mut CallableEnv,
    ) -> (Changes, BTreeSet<String>) {
        let mut declared = BTreeSet::new();
        let mut changes = Changes::new();
        for statement in statements {
            let writes = self.assignment_statement(statement, scope, env);
            merge_changes(&mut changes, visible_changes(writes, &declared));
            if let Stmt::Let { name, .. } = statement {
                let _ = declared.insert(name.clone());
            }
        }
        (changes, declared)
    }

    fn assignment_statement(
        &self,
        statement: &Stmt,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Changes {
        match statement {
            Stmt::Let { name, value, .. } => self.assignment_binding(name, value, scope, env),
            Stmt::Assignment { name, value, .. } => self.assignment_write(name, value, scope, env),
            Stmt::Expr { value, .. } => self.flow_callable_assignments(value, scope, env),
            _ => Changes::new(),
        }
    }

    fn assignment_binding(
        &self,
        name: &str,
        value: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Changes {
        let changes = self.flow_callable_assignments(value, scope, env);
        self.rebind_value(name, value, scope, env);
        let _ = env.shadowed.insert(name.to_owned());
        changes
    }

    fn assignment_write(
        &self,
        name: &str,
        value: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Changes {
        let mut changes = self.flow_callable_assignments(value, scope, env);
        let incoming = self
            .value(value, scope, env)
            .unwrap_or_else(Value::unknown_value);
        let slot = env
            .values
            .entry(name.to_owned())
            .or_insert_with(Value::unknown_value);
        slot.union(incoming);
        let _ = changes.insert(name.to_owned(), slot.clone());
        changes
    }

    fn assignment_send(
        &self,
        channel: &Expr,
        value: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Changes {
        self.channel_payload_assignment(channel, value, scope, env);
        let mut changes = self.flow_callable_assignments(channel, scope, env);
        merge_changes(
            &mut changes,
            self.flow_callable_assignments(value, scope, env),
        );
        changes
    }

    fn channel_payload_assignment(
        &self,
        channel: &Expr,
        value: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) {
        let sites = self
            .value(channel, scope, env)
            .map(|channel| channel.channel_sites)
            .unwrap_or_default();
        let payload = self
            .value(value, scope, env)
            .unwrap_or_else(Value::unknown_value);
        for site in sites {
            merge_value(
                env.channel_payloads.entry(site).or_default(),
                payload.clone(),
            );
        }
    }
}

fn merge_channels(target: &mut CallableEnv, source: CallableEnv) {
    for (site, payload) in source.channel_payloads {
        merge_value(target.channel_payloads.entry(site).or_default(), payload);
    }
}

fn executed_children(expression: &Expr) -> Vec<&Expr> {
    let mut children = Vec::new();
    walk_children(expression, |child| {
        children.push(executed_child(expression, child));
    });
    children
}

fn executed_child<'a>(parent: &'a Expr, child: &'a Expr) -> &'a Expr {
    match parent {
        Expr::Handler {
            return_clause: Some(clause),
            ..
        } if std::ptr::eq(child, clause.as_ref()) => match clause.as_ref() {
            Expr::Lambda { body, .. } => body,
            _ => child,
        },
        _ => child,
    }
}

fn visible_changes(mut changes: Changes, declared: &BTreeSet<String>) -> Changes {
    changes.retain(|name, _| !declared.contains(name));
    changes
}
