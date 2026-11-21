//! Whether Osprey's inference alone gives a definition its Lean type, so the
//! generated module can leave the `name : type` header off ([MATH-CODE-EMIT-ELEGANT]).
//!
//! Every value of the fragment is `Float` or `Bool`. A value is concrete once it
//! is unified with something whose type is fixed: a literal, a boolean operator,
//! an annotated parameter, a concrete parameter or result of an earlier
//! definition, or an operand of a float builtin (`floatDiv`, `sqrt`, …), which
//! unifies its operands and result. Arithmetic and comparison unify their
//! operands only when one is already concrete; between two open operands they
//! defer, and the deferred values stay generic in the definition's scheme.
//!
//! Within its own definition a value is settled when it is concrete or reaches,
//! through deferred operators, something concrete or obliged to be a float.
//! A value passed to a generic parameter of an earlier definition must be
//! concrete: the checker defaults such a value to `int`.

use crate::model::{Def, Expr, Op, Tail, Ty};

const FLOAT: usize = 0;
const BOOL: usize = 1;

/// Whether each parameter, then the result, of a definition is concrete in its emitted scheme.
type Concrete = Vec<bool>;

/// The fewest parameters to annotate inline, first to last, so the body settles every other
/// parameter and the result; `None` when the result stays open and only a header will do.
pub(crate) fn annotated(def: &Def, earlier: &[Def]) -> Option<Vec<usize>> {
    let mut known: Vec<Concrete> = Vec::with_capacity(earlier.len());
    for (index, prior) in earlier.iter().enumerate() {
        let before = earlier.get(..index).unwrap_or_default();
        let concrete = concreteness(prior, before, &known);
        known.push(concrete);
    }
    chosen(def, earlier, &known)
}

fn chosen(def: &Def, earlier: &[Def], known: &[Concrete]) -> Option<Vec<usize>> {
    let mut chosen = Vec::new();
    loop {
        let pins = walked(def, earlier, known, &chosen);
        let open = (0..=def.params.len()).find(|index| !pins.settled(*index));
        match open {
            None => return Some(chosen),
            Some(index) if index < def.params.len() => chosen.push(index),
            Some(_) => return None,
        }
    }
}

/// A definition's emitted scheme: with a header every type is concrete; otherwise only what
/// its annotations and body unify with a fixed type.
fn concreteness(def: &Def, earlier: &[Def], known: &[Concrete]) -> Concrete {
    let Some(annotations) = chosen(def, earlier, known) else {
        return vec![true; def.params.len() + 1];
    };
    let pins = walked(def, earlier, known, &annotations);
    (0..=def.params.len())
        .map(|index| pins.concrete(pins.head(index)))
        .collect()
}

/// The body walked with the `chosen` parameters carrying their Lean type.
fn walked<'a>(def: &Def, earlier: &'a [Def], known: &'a [Concrete], chosen: &[usize]) -> Pins<'a> {
    let mut pins = Pins {
        roots: vec![FLOAT, BOOL],
        names: Vec::new(),
        obliged: Vec::new(),
        deferred: Vec::new(),
        generic: Vec::new(),
        heads: Vec::new(),
        earlier,
        known,
    };
    for (index, (name, ty)) in def.params.iter().enumerate() {
        let node = pins.local(name);
        pins.heads.push(node);
        if chosen.contains(&index) {
            pins.join(node, of(*ty));
        }
    }
    let result = pins.tail(&def.body);
    pins.heads.push(result);
    pins
}

struct Pins<'a> {
    roots: Vec<usize>,
    names: Vec<(String, usize)>,
    /// Nodes a float builtin obliges to be a float.
    obliged: Vec<usize>,
    /// Pairs an operator between two open operands relates without unifying.
    deferred: Vec<(usize, usize)>,
    /// Nodes passed to a generic parameter of an earlier definition.
    generic: Vec<usize>,
    /// The parameters' nodes, then the result's.
    heads: Vec<usize>,
    earlier: &'a [Def],
    known: &'a [Concrete],
}

impl Pins<'_> {
    fn head(&self, index: usize) -> usize {
        self.heads.get(index).copied().unwrap_or(FLOAT)
    }

    fn fresh(&mut self) -> usize {
        self.roots.push(self.roots.len());
        self.roots.len() - 1
    }

    fn local(&mut self, name: &str) -> usize {
        let node = self.fresh();
        self.names.push((name.to_owned(), node));
        node
    }

    fn root(&self, mut node: usize) -> usize {
        while let Some(&parent) = self.roots.get(node).filter(|parent| **parent != node) {
            node = parent;
        }
        node
    }

    /// Join two nodes; a fixed type is always the root it joins under.
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        let (root, child) = if b <= BOOL { (b, a) } else { (a, b) };
        if let Some(slot) = self.roots.get_mut(child) {
            *slot = root;
        }
    }

    /// Join two nodes and name what they now are.
    fn joined(&mut self, a: usize, b: usize) -> usize {
        self.join(a, b);
        self.root(a)
    }

    fn concrete(&self, node: usize) -> bool {
        self.root(node) <= BOOL
    }

    fn marked(&self, marks: &[usize], node: usize) -> bool {
        let root = self.root(node);
        marks.iter().any(|mark| self.root(*mark) == root)
    }

    /// Whether the parameter or result at `index` gets its Lean type in this definition.
    fn settled(&self, index: usize) -> bool {
        let node = self.head(index);
        if self.marked(&self.generic, node) {
            return self.concrete(node);
        }
        self.concrete(node) || self.evidenced(node)
    }

    /// Whether `node` reaches, through deferred operators, a concrete or float-obliged value.
    fn evidenced(&self, node: usize) -> bool {
        let mut reached = vec![self.root(node)];
        let mut grew = true;
        while grew {
            grew = false;
            for (a, b) in &self.deferred {
                let (a, b) = (self.root(*a), self.root(*b));
                for (from, to) in [(a, b), (b, a)] {
                    if reached.contains(&from) && !reached.contains(&to) {
                        reached.push(to);
                        grew = true;
                    }
                }
            }
        }
        reached
            .iter()
            .any(|root| *root <= BOOL || self.marked(&self.obliged, *root))
    }

    fn tail(&mut self, tail: &Tail) -> usize {
        match tail {
            Tail::Expr(term) => self.expr(term),
            Tail::If(test, yes, no) => {
                let test = self.expr(test);
                self.join(test, BOOL);
                let (yes, no) = (self.tail(yes), self.tail(no));
                self.related(yes, no)
            }
            Tail::Let(name, value, body) => {
                let value = self.tail(value);
                let node = self.local(name);
                self.join(node, value);
                let result = self.tail(body);
                self.names.truncate(self.names.len().saturating_sub(1));
                result
            }
        }
    }

    fn expr(&mut self, term: &Expr) -> usize {
        match term {
            Expr::Num(_) => FLOAT,
            Expr::Bool(_) => BOOL,
            Expr::Name(name) => self.named(name),
            Expr::Call(callee, arguments) => self.call(callee, arguments),
            Expr::Not(operand) => {
                let operand = self.expr(operand);
                self.joined(operand, BOOL)
            }
            Expr::Binary(op, left, right) => self.binary(*op, left, right),
        }
    }

    fn named(&mut self, name: &str) -> usize {
        match self.names.iter().rev().find(|(local, _)| local == name) {
            Some((_, node)) => *node,
            None => self.fresh(),
        }
    }

    /// Unify two operands when one is concrete; otherwise relate them, and a fresh result, by deferral.
    fn related(&mut self, left: usize, right: usize) -> usize {
        if self.concrete(left) || self.concrete(right) {
            return self.joined(left, right);
        }
        let result = self.fresh();
        self.deferred.extend([(left, right), (left, result)]);
        result
    }

    fn binary(&mut self, op: Op, left: &Expr, right: &Expr) -> usize {
        let (left, right) = (self.expr(left), self.expr(right));
        match op {
            Op::Or | Op::And => {
                self.join(left, BOOL);
                self.joined(right, BOOL)
            }
            Op::Div => self.builtin(&[left, right]),
            Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Eq | Op::Ne => {
                let _compared = self.related(left, right);
                BOOL
            }
            Op::Add | Op::Sub | Op::Mul => self.related(left, right),
        }
    }

    /// A float builtin unifies its operands and result, each obliged to be a float.
    fn builtin(&mut self, operands: &[usize]) -> usize {
        let result = operands
            .iter()
            .fold(self.fresh(), |shared, node| self.joined(shared, *node));
        self.obliged.push(result);
        result
    }

    /// A definition takes and returns its emitted types: a concrete parameter fixes its argument,
    /// and a generic one marks it. `abs` is open over `int` too, so it fixes nothing.
    fn call(&mut self, callee: &str, arguments: &[Expr]) -> usize {
        let nodes: Vec<usize> = arguments
            .iter()
            .map(|argument| self.expr(argument))
            .collect();
        let found = self.earlier.iter().position(|def| def.name == callee);
        if let Some((def, concrete)) =
            found.and_then(|at| self.earlier.get(at).zip(self.known.get(at)))
        {
            return self.instantiated(def, concrete, &nodes);
        }
        if callee == "abs" {
            return nodes.first().copied().unwrap_or(FLOAT);
        }
        self.builtin(&nodes)
    }

    fn instantiated(&mut self, def: &Def, concrete: &[bool], nodes: &[usize]) -> usize {
        for ((node, (_, ty)), fixed) in nodes.iter().zip(&def.params).zip(concrete) {
            if *fixed {
                self.join(*node, of(*ty));
            } else {
                self.generic.push(*node);
            }
        }
        match concrete.get(def.params.len()) {
            Some(true) => of(def.result),
            _ => self.fresh(),
        }
    }
}

fn of(ty: Ty) -> usize {
    match ty {
        Ty::Float => FLOAT,
        Ty::Bool => BOOL,
    }
}
