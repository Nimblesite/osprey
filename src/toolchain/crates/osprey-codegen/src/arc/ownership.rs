//! The ledger's checking mode: compile-time ownership [MEM-OWNERSHIP-STATIC]
//! (docs/specs/compiler/0018-MemoryManagement.md). Every reference the ledger
//! cannot move it would count at run time; under `--memory=ownership` each of
//! those is reported instead, and a program with one is refused.
use super::Codegen;
use crate::llty::Value;
use osprey_ast::Position;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// The symbol prefix of every helper function the compiler synthesises.
const SYNTHESISED: &str = "osp.";

/// Refuse, from the next compilation on, every program that needs a second
/// owner for a managed value [MEM-OWNERSHIP-STATIC].
pub fn set_ownership_check(on: bool) {
    REQUESTED.store(on, Ordering::Relaxed);
}

/// Why a value needs a reference beyond the one its owner holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Need {
    /// It is read out of a temporary or a replaceable binding and given a name.
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
    /// It is passed to a function that takes ownership of that parameter.
    Given,
    /// A new version of a list or map is derived from it while it is still read.
    Versioned,
}

impl Need {
    fn reason(self) -> &'static str {
        match self {
            Need::Bound => "it is bound to a name that can outlive the value it was read from",
            Need::Stored => "it is stored in a new value while its owner keeps it",
            Need::Returned => "it is returned while another value owns it",
            Need::Kept => "it is handed to a fiber, channel, handler or collection that keeps it",
            Need::Joined => "one arm of a `match` yields it while another arm yields a new value",
            Need::Shared => "a file-scope binding or cell holds it as well as its owner",
            Need::Given => "it is given to a function that takes ownership of it",
            Need::Versioned => {
                "a new version of the collection is derived from it while another owner keeps it"
            }
        }
    }
}

/// The other use that keeps a named value from moving to the one reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherUse {
    /// A later statement reads it, here when the statement has a position.
    ReadLater(Option<Position>),
    /// The same statement reads it again, or reads it where it may not run.
    ReadAgainHere,
}

impl fmt::Display for OtherUse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtherUse::ReadLater(Some(at)) => {
                write!(f, "it is read again at {}:{}", at.line, at.column)
            }
            OtherUse::ReadLater(None) => write!(f, "it is read again later"),
            OtherUse::ReadAgainHere => write!(
                f,
                "this statement reads it more than once or on a path that may not run"
            ),
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
    /// What else uses the value, when the ledger knows.
    pub other: Option<OtherUse>,
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
        write!(f, " in `{}`: {}", self.function, self.need.reason())?;
        if let Some(other) = self.other {
            write!(f, "; {other}")?;
        }
        write!(f, " [MEM-OWNERSHIP-STATIC]")
    }
}

/// What the check has seen so far in this module.
#[derive(Debug)]
pub(crate) struct OwnershipCheck {
    pub(crate) on: bool,
    pub(crate) position: Option<Position>,
    pub(crate) function: String,
    pub(crate) found: Vec<SecondOwner>,
    /// Which parameters each function owns [MEM-OWNERSHIP-STATIC-PARAMETERS].
    pub(crate) owned_parameters: super::OwnedParameters,
    /// Where each closure that lends its captures is written, as line and
    /// column [MEM-OWNERSHIP-STATIC-CLOSURES].
    pub(crate) lent_closures: std::collections::BTreeSet<(u32, u32)>,
    /// Where each list literal that only lends its elements is written.
    pub(crate) lent_lists: std::collections::BTreeSet<(u32, u32)>,
    /// Whether the next Result wraps an element read out of a collection.
    pub(crate) reading: bool,
    /// Where each suspended function's nested function began in `found`.
    nested: Vec<usize>,
}

impl OwnershipCheck {
    /// A check that is on when `--memory=ownership` asked for it.
    pub(crate) fn requested() -> OwnershipCheck {
        OwnershipCheck {
            on: REQUESTED.load(Ordering::Relaxed),
            position: None,
            function: String::new(),
            found: Vec::new(),
            owned_parameters: super::OwnedParameters::new(),
            lent_closures: std::collections::BTreeSet::new(),
            lent_lists: std::collections::BTreeSet::new(),
            reading: false,
            nested: Vec::new(),
        }
    }

    /// A nested function starts: what it reports is judged when it ends.
    pub(crate) fn enter_nested(&mut self) {
        self.nested.push(self.found.len());
    }

    /// The nested function `name` ended. A helper the compiler synthesises
    /// counts references among its own temporaries, which no source value
    /// needs, so what it reported is dropped.
    pub(crate) fn leave_nested(&mut self, name: &str) {
        let start = self.nested.pop().unwrap_or(self.found.len());
        if name.starts_with(SYNTHESISED) {
            self.found.truncate(start);
        }
    }

    /// The statement being lowered, kept when the new one has no position.
    pub(crate) fn at(&mut self, position: Option<Position>) {
        self.position = position.or(self.position);
    }
}

/// The ledger is about to count a reference to `value`: record it, unless it
/// is a fiber or channel handle. A handle is the one value a program may share
/// under ownership: fibers exist to be awaited and channels to be shared, so
/// the runtime counts a handle's sharers itself and the last one to free it
/// ends it [MEM-OWNERSHIP-STATIC-HANDLES].
pub(crate) fn second_owner_of(cg: &mut Codegen, value: &Value, name: Option<&str>, need: Need) {
    if value.fiber_elem.is_none() {
        second_owner(cg, &value.operand, name, need);
    }
}

/// The ledger is about to count a reference to `operand`: record it, unless a
/// binding names it as a handle ([`second_owner_of`]).
pub(crate) fn second_owner(cg: &mut Codegen, operand: &str, name: Option<&str>, need: Need) {
    if !cg.ownership.on
        || !operand.starts_with('%')
        || cg.is_rodata(operand)
        || cg.names_handle(operand)
    {
        return;
    }
    let named = name.map(str::to_owned).or_else(|| cg.name_of(operand));
    let value = named.or_else(|| super::lends::lent_name(cg, operand));
    let other = value
        .as_deref()
        .and_then(|name| super::moves::other_use(cg, name));
    let found = SecondOwner {
        position: cg.ownership.position,
        function: osprey_ast::symbol::demangle_message(&cg.ownership.function).into_owned(),
        value,
        need,
        other,
    };
    if !cg.ownership.found.contains(&found) {
        cg.ownership.found.push(found);
    }
}

/// The calls that count a reference. The ledger model has no such action
/// [MEM-OWNERSHIP-STATIC-MODEL], so accepted code may not contain one. A shared
/// handle is counted by `osp_ownership_share` instead
/// [MEM-OWNERSHIP-STATIC-HANDLES].
const COUNTS: [&str; 3] = [
    "call void @osp_retain(",
    "call void @osp_release(",
    "call void @osp_release_unique(",
];

/// `ir` for an accepted module: under ownership it frees and never counts
/// [MEM-OWNERSHIP-STATIC-SAFETY]. A count that got past the ledger is a
/// compiler fault, reported instead of built.
fn counts_nothing(cg: &Codegen, ir: String) -> crate::error::Result<String> {
    if cg.ownership.on && COUNTS.iter().any(|call| ir.contains(call)) {
        return Err(crate::error::CodegenError::invalid(
            "ownership: the lowering counted a reference the ledger did not refuse [MEM-OWNERSHIP-STATIC]",
        ));
    }
    Ok(ir)
}

/// The refusal for a module that needed second owners, else `ir` unchanged.
pub(crate) fn verdict(cg: &Codegen, ir: String) -> crate::error::Result<String> {
    if cg.ownership.found.is_empty() {
        return counts_nothing(cg, ir);
    }
    let mut found = cg.ownership.found.clone();
    found.sort_by_key(|one| one.position.map(|at| (at.line, at.column)));
    Err(crate::error::CodegenError::Ownership(found))
}
