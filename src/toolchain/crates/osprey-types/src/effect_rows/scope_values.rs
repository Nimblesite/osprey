//! The names a [`super::CallableEnv`] binds to values, layered so a scope is
//! cheap to copy: every block, lambda and body starts from its enclosing
//! scope, and the file scope alone holds every top-level value. A copy shares
//! the layers beneath it and owns only the names bound since.

use super::{BTreeSet, HashMap, Value};
use std::sync::Arc;

/// A value map whose copies share everything bound before they were made.
#[derive(Clone, Default)]
pub(super) struct ScopeValues {
    /// Bindings shared with every copy made after they were frozen.
    shared: Arc<HashMap<String, Value>>,
    /// Bindings since: `None` hides a shared name.
    own: HashMap<String, Option<Value>>,
}

impl ScopeValues {
    pub(super) fn get(&self, name: &str) -> Option<&Value> {
        if let Some(own) = self.own.get(name) {
            return own.as_ref();
        }
        super::worklist::record_scope_read(name);
        self.shared.get(name)
    }

    /// The names bound to a different value here than in `other`.
    pub(super) fn changed_names(&self, other: &ScopeValues) -> BTreeSet<String> {
        let mine = self.entries().filter(|(n, v)| other.get(n) != Some(*v));
        let theirs = other.entries().filter(|(n, _)| self.get(n).is_none());
        mine.chain(theirs).map(|(n, _)| n.clone()).collect()
    }

    pub(super) fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub(super) fn insert(&mut self, name: String, value: Value) -> Option<Value> {
        let previous = self.get(&name).cloned();
        let _ = self.own.insert(name, Some(value));
        previous
    }

    pub(super) fn remove(&mut self, name: &str) -> Option<Value> {
        let previous = self.get(name).cloned();
        if previous.is_some() {
            let _ = self.own.insert(name.to_owned(), None);
        }
        previous
    }

    #[cfg(test)]
    pub(super) fn extend(&mut self, values: impl IntoIterator<Item = (String, Value)>) {
        for (name, value) in values {
            let _ = self.insert(name, value);
        }
    }

    /// Join each change into the value already bound, or bind it.
    pub(super) fn merge(&mut self, changes: HashMap<String, Value>) {
        for (name, value) in changes {
            let joined = match self.get(&name) {
                Some(current) => {
                    let mut current = current.clone();
                    current.union(value);
                    current
                }
                None => value,
            };
            let _ = self.own.insert(name, Some(joined));
        }
    }

    /// Rewrite every bound value; the result shares nothing with its source.
    pub(super) fn map_values(&mut self, mut rewrite: impl FnMut(&mut Value)) {
        let mut all: HashMap<String, Value> = self
            .entries()
            .map(|(n, v)| (n.clone(), v.clone()))
            .collect();
        all.values_mut().for_each(&mut rewrite);
        *self = ScopeValues {
            shared: Arc::new(all),
            own: HashMap::new(),
        };
    }

    /// Fold this scope's own bindings into a layer its copies share, so a
    /// scope every body starts from (the file scope) is never copied whole.
    pub(super) fn frozen(mut self) -> ScopeValues {
        if self.own.is_empty() {
            return self;
        }
        let mut shared = Arc::unwrap_or_clone(self.shared);
        for (name, value) in self.own.drain() {
            match value {
                Some(value) => {
                    let _ = shared.insert(name, value);
                }
                None => {
                    let _ = shared.remove(&name);
                }
            }
        }
        ScopeValues {
            shared: Arc::new(shared),
            own: HashMap::new(),
        }
    }

    /// Every visible binding.
    fn entries(&self) -> impl Iterator<Item = (&String, &Value)> {
        let own = self
            .own
            .iter()
            .filter_map(|(n, v)| v.as_ref().map(|v| (n, v)));
        let shared = self
            .shared
            .iter()
            .filter(|(n, _)| !self.own.contains_key(*n))
            .inspect(|(n, _)| super::worklist::record_scope_read(n));
        own.chain(shared)
    }

    fn visible_len(&self) -> usize {
        self.entries().count()
    }
}

impl PartialEq for ScopeValues {
    fn eq(&self, other: &ScopeValues) -> bool {
        self.visible_len() == other.visible_len()
            && self.entries().all(|(n, v)| other.get(n) == Some(v))
    }
}

impl Eq for ScopeValues {}
