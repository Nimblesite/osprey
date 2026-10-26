//! Fiber, immutable-update and Result-default spelling preserves the shared
//! runtime contract.
//! Implements [FLAVOR-PYTHON-INTEROP], [FLAVOR-PYTHON-EXPRESSIONS].
use super::context::{apply, position, Lowerer, Result};
use osprey_ast::{Expr, FieldAssignment, Stmt};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn fiber_intrinsic(&mut self, node: Node<'_>, name: &str) -> Result<Option<Expr>> {
        if name == "update" {
            return self.update(node).map(Some);
        }
        if name == "unwrap_or" {
            return self.unwrap_or(node).map(Some);
        }
        if !matches!(
            name,
            "spawn" | "await_fiber" | "yield_fiber" | "send" | "recv"
        ) {
            return Ok(None);
        }
        let (args, named) = self.arguments(Self::field(node, "arguments")?)?;
        if !named.is_empty() {
            return self.unsupported(node);
        }
        self.fiber_call(node, name, &args).map(Some)
    }

    fn fiber_call(&self, node: Node<'_>, name: &str, args: &[Expr]) -> Result<Expr> {
        match (name, args) {
            ("spawn", [work]) => Ok(Expr::Spawn(Box::new(apply(work.clone(), Vec::new())))),
            ("await_fiber", [fiber]) => Ok(Expr::Await(Box::new(fiber.clone()))),
            ("yield_fiber", []) => Ok(Expr::Yield(None)),
            ("yield_fiber", [value]) => Ok(Expr::Yield(Some(Box::new(value.clone())))),
            ("recv", [channel]) => Ok(Expr::Recv(Box::new(channel.clone()))),
            ("send", [channel, value]) => Ok(Expr::Send {
                channel: Box::new(channel.clone()),
                value: Box::new(value.clone()),
            }),
            _ => self.unsupported(node),
        }
    }

    /// `unwrap_or(result, fallback)` is Default `result ?: fallback`: the
    /// fallback is the Error arm, evaluated only when there is no value.
    fn unwrap_or(&mut self, node: Node<'_>) -> Result<Expr> {
        let (args, named) = self.arguments(Self::field(node, "arguments")?)?;
        match (args.as_slice(), named.is_empty()) {
            ([result, fallback], true) => Ok(crate::desugar::result_default(
                result.clone(),
                fallback.clone(),
            )),
            _ => self.unsupported(node),
        }
    }

    fn update(&mut self, node: Node<'_>) -> Result<Expr> {
        let (args, named) = self.arguments(Self::field(node, "arguments")?)?;
        let [record] = args.as_slice() else {
            return self.unsupported(node);
        };
        let fields = super::expressions::fields(named);
        let Expr::Identifier(name) = record else {
            return Ok(self.bound_update(node, record.clone(), fields));
        };
        Ok(Expr::Update {
            record: name.clone(),
            fields,
        })
    }

    /// Bind an arbitrary record expression once, then update the binding: the
    /// canonical `Expr::Update` names a record variable, never an expression
    /// ([TYPE-RECORD-UPDATE]).
    fn bound_update(&mut self, node: Node<'_>, record: Expr, fields: Vec<FieldAssignment>) -> Expr {
        let name = self.fresh("upd");
        crate::desugar::block(
            vec![Stmt::Let {
                name: name.clone(),
                mutable: false,
                ty: None,
                value: record,
                doc: None,
                position: None,
            }],
            Some(Box::new(Expr::Update {
                record: name,
                fields,
            })),
            Some(position(node)),
        )
    }
}
