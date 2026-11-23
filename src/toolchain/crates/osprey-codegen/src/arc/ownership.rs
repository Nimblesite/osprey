//! The ledger's checking mode: compile-time ownership [MEM-OWNERSHIP-STATIC]
//! (docs/specs/compiler/0018-MemoryManagement.md). Every reference the ledger
//! cannot move it would count at run time; under `--memory=ownership` each of
//! those is reported instead, and a program with one is refused.
use super::Codegen;
use osprey_ast::Position;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Refuse, from the next compilation on, every program that needs a second
/// owner for a managed value [MEM-OWNERSHIP-STATIC].
pub fn set_ownership_check(on: bool) {
    REQUESTED.store(on, Ordering::Relaxed);
}

/// Why a value needs a reference beyond the one its owner holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Need {
    /// A value something else owns is given a name of its own.
    Bound,
    /// It is stored in a new value while its owner keeps it.
    Stored,
    /// It is returned while the caller or another value still owns it.
    Returned,
    /// It crosses to a fiber, channel, handler or collection that keeps it.
    Kept,
    /// One arm of a `match` yields it where another arm yields a new value.
    Joined,
    /// A file-scope binding or a cell holds it as well as its owner.
    Shared,
}

impl Need {
    fn reason(self) -> &'static str {
        match self {
            Need::Bound => "it is bound to a new name while another value owns it",
            Need::Stored => "it is stored in a new value while its owner keeps it",
            Need::Returned => "it is returned while another value owns it",
            Need::Kept => "it is handed to a fiber, channel, handler or collection that keeps it",
            Need::Joined => "one arm of a `match` yields it while another arm yields a new value",
            Need::Shared => "a file-scope binding or cell holds it as well as its owner",
        }
    }
}

/// One value the ledger could not give a single owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondOwner {
    /// The statement that needs the reference.
    pub position: Option<Position>,
    /// The enclosing function, as written.
    pub function: String,
    /// The binding that names the value, when one does.
    pub value: Option<String>,
    /// Why the reference is needed.
    pub need: Need,
}

impl fmt::Display for SecondOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(at) = self.position {
            write!(f, "{}:{}: ", at.line, at.column)?;
        }
        write!(f, "ownership: ")?;
        match &self.value {
            Some(name) => write!(f, "`{name}` needs a second owner")?,
            None => write!(f, "a value needs a second owner")?,
        }
        write!(
            f,
            " in `{}`: {} [MEM-OWNERSHIP-STATIC]",
            self.function,
            self.need.reason()
        )
    }
}

/// What the check has seen so far in this module.
#[derive(Debug)]
pub(crate) struct OwnershipCheck {
    pub(crate) on: bool,
    pub(crate) position: Option<Position>,
    pub(crate) function: String,
    pub(crate) found: Vec<SecondOwner>,
}

impl OwnershipCheck {
    /// A check that is on when `--memory=ownership` asked for it.
    pub(crate) fn requested() -> OwnershipCheck {
        OwnershipCheck {
            on: REQUESTED.load(Ordering::Relaxed),
            position: None,
            function: String::new(),
            found: Vec::new(),
        }
    }

    /// The statement being lowered, kept when the new one has no position.
    pub(crate) fn at(&mut self, position: Option<Position>) {
        self.position = position.or(self.position);
    }
}

/// The ledger is about to count a reference to `operand`: record it.
#[track_caller]
pub(crate) fn second_owner(cg: &mut Codegen, operand: &str, name: Option<&str>, need: Need) {
    if !cg.ownership.on || !operand.starts_with('%') || cg.is_rodata(operand) {
        return;
    }
    let value = name.map(str::to_owned).or_else(|| cg.name_of(operand));
    let found = SecondOwner {
        position: cg.ownership.position,
        function: format!("{}#{}", std::panic::Location::caller().file(), std::panic::Location::caller().line()),
        value,
        need,
    };
    if !cg.ownership.found.contains(&found) {
        cg.ownership.found.push(found);
    }
}

/// The refusal for a module that needed second owners, else `ir` unchanged.
pub(crate) fn verdict(cg: &Codegen, ir: String) -> crate::error::Result<String> {
    if cg.ownership.found.is_empty() {
        return Ok(ir);
    }
    let mut found = cg.ownership.found.clone();
    found.sort_by_key(|one| one.position.map(|at| (at.line, at.column)));
    Err(crate::error::CodegenError::Ownership(found))
}
