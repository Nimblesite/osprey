//! What the escape analysis records: the captured requests an expression can
//! leave unanswered, the names visible where it stands, and the solved
//! program-wide facts.

use super::table::Table;
use osprey_ast::{effect_name, HandlerArm};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The captured requests an expression can leave unanswered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Esc {
    /// Captured effects requested here and answered by no handler here.
    pub(super) fixed: BTreeSet<String>,
    /// Parameters of the enclosing function whose requests pass through, each
    /// with the effects a handler here already answers.
    pub(super) params: BTreeMap<usize, BTreeSet<String>>,
}

impl Esc {
    pub(super) fn is_empty(&self) -> bool {
        self.fixed.is_empty() && self.params.is_empty()
    }

    pub(super) fn param(index: usize) -> Self {
        Self {
            fixed: BTreeSet::new(),
            params: BTreeMap::from([(index, BTreeSet::new())]),
        }
    }

    pub(super) fn absorb(&mut self, other: Self) {
        self.fixed.extend(other.fixed);
        for (index, answered) in other.params {
            let merged = match self.params.get(&index) {
                Some(mine) => mine.intersection(&answered).cloned().collect(),
                None => answered,
            };
            let _ = self.params.insert(index, merged);
        }
    }

    /// What is left once a handler for `effect` encloses this.
    pub(super) fn answered(mut self, effect: &str) -> Self {
        let _ = self.fixed.remove(effect);
        for hidden in self.params.values_mut() {
            let _ = hidden.insert(effect.to_owned());
        }
        self
    }

    /// This set as it leaves a callee that already answers `hidden`.
    pub(super) fn behind(mut self, hidden: &BTreeSet<String>) -> Self {
        self.fixed.retain(|effect| !hidden.contains(effect));
        for own in self.params.values_mut() {
            own.extend(hidden.iter().cloned());
        }
        self
    }
}

/// A lambda value: what calling it leaves unanswered and its arity, plus its
/// function id when a `let` binds it.
#[derive(Clone, Debug)]
pub(super) struct Alias {
    pub(super) id: Option<usize>,
    pub(super) latent: Esc,
    pub(super) arity: usize,
}

/// The names visible at one point of one function body.
#[derive(Clone, Debug, Default)]
pub(super) struct Scope {
    pub(super) func: Option<usize>,
    pub(super) namespace: Vec<String>,
    pub(super) params: HashMap<String, usize>,
    pub(super) aliases: HashMap<String, Alias>,
    pub(super) hidden: HashSet<String>,
    /// What `resume(..)` in the enclosing arm can leave unanswered.
    pub(super) resume: Esc,
    /// Whether the enclosing arm answers a captured effect, so its `resume`
    /// is a closure call.
    pub(super) resume_captured: bool,
    /// The closure a rewritten arm's `resume` calls.
    pub(super) resume_name: Option<String>,
}

impl Scope {
    pub(super) fn of_function(table: &Table<'_>, id: usize) -> Self {
        let Some(func) = table.func(id) else {
            return Self::default();
        };
        Self {
            func: Some(id),
            namespace: func.scope.clone(),
            params: func.params.iter().cloned().zip(0..).collect(),
            ..Self::default()
        }
    }

    pub(super) fn top(namespace: &[String]) -> Self {
        Self {
            namespace: namespace.to_vec(),
            ..Self::default()
        }
    }

    /// This scope with `names` bound to plain local values.
    pub(super) fn binding(&self, names: impl IntoIterator<Item = String>) -> Self {
        let mut inner = self.clone();
        for name in names {
            let _ = inner.params.remove(&name);
            let _ = inner.aliases.remove(&name);
            let _ = inner.hidden.insert(name);
        }
        inner
    }

    pub(super) fn with_alias(&self, name: &str, alias: Alias) -> Self {
        let mut inner = self.binding([name.to_owned()]);
        let _ = inner.aliases.insert(name.to_owned(), alias);
        inner
    }

    /// The scope of the body of `let`-bound lambda `id`: its parameters are
    /// that function's parameters, and the enclosing function's are plain names.
    pub(super) fn entering(&self, id: usize, params: &[String]) -> Self {
        let mut inner = self.binding(self.params.keys().cloned().collect::<Vec<_>>());
        inner.func = Some(id);
        inner.params = params.iter().cloned().zip(0..).collect();
        for name in params {
            let _ = inner.aliases.remove(name);
            let _ = inner.hidden.remove(name);
        }
        inner
    }

    /// The scope of one handler arm's body.
    pub(super) fn in_arm(&self, arm: &HandlerArm, resume: Esc, captured: bool) -> Self {
        let mut inner = self.binding(arm.params.iter().cloned());
        inner.resume = resume;
        inner.resume_captured = captured;
        inner.resume_name = None;
        inner
    }
}

/// What a call's callee expression names.
#[derive(Debug)]
pub(super) enum Callee {
    Function(usize),
    Alias(Alias),
    Param(usize),
    Other,
}

/// The solved program-wide facts.
#[derive(Clone, Debug, Default)]
pub(super) struct Facts {
    /// Effects whose requests carry their continuation.
    pub(super) captured: BTreeSet<String>,
    /// `(function, parameter)` pairs that receive continuation-capturing callables.
    pub(super) stepping: BTreeSet<(usize, usize)>,
    pub(super) summaries: Vec<Esc>,
}

impl Facts {
    pub(super) fn is_captured(&self, mention: &str) -> bool {
        self.captured.contains(effect_name::base(mention))
    }

    pub(super) fn summary(&self, id: usize) -> Esc {
        self.summaries.get(id).cloned().unwrap_or_default()
    }

    /// Whether calling function `id` yields a step rather than a plain value.
    pub(super) fn returns_step(&self, id: usize) -> bool {
        self.summaries.get(id).is_some_and(|esc| !esc.is_empty())
    }

    /// Whether parameter `index` of function `id` holds a step-returning callable.
    pub(super) fn takes_step(&self, id: usize, index: usize) -> bool {
        self.stepping.contains(&(id, index))
    }
}
