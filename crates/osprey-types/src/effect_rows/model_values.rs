//! Effect row model values.
use super::{merge_boxed_value, merge_callable, widen_value, BTreeSet, Callable, Summary, Value};

impl Value {
    pub(super) fn child_count(&self) -> usize {
        self.fields.len()
            + usize::from(self.element.is_some())
            + usize::from(self.result_payload.is_some())
            + usize::from(self.fiber_payload.is_some())
            + usize::from(self.callable.is_some())
            + usize::from(!self.deferred.parameter_uses.is_empty())
    }

    pub(super) fn carries_provenance(&self) -> bool {
        self.callable.is_some()
            || !self.performed.is_empty()
            || !self.fields.is_empty()
            || self.element.is_some()
            || self.result_payload.is_some()
            || self.fiber_payload.is_some()
            || !self.channel_sites.is_empty()
            || self.deferred != Summary::default()
    }

    pub(super) fn from_callable(callable: Callable) -> Self {
        Self {
            field_names: matches!(callable, Callable::Known(_)).then(BTreeSet::new),
            callable: Some(callable),
            ..Self::default()
        }
    }

    pub(super) fn parameter(level: usize, index: usize) -> Self {
        Self::from_callable(Callable::Parameter {
            level,
            index,
            projection: Vec::new(),
        })
    }

    pub(super) fn unknown_callable() -> Self {
        Self::from_callable(Callable::Unknown)
    }

    pub(super) fn widened(self) -> Self {
        widen_value(self, 0)
    }

    pub(super) fn channel(site: usize) -> Self {
        Self {
            channel_sites: [site].into_iter().collect(),
            field_names: Some(BTreeSet::new()),
            ..Self::default()
        }
    }

    pub(super) fn union(&mut self, other: Self) {
        if self.field_names != other.field_names {
            self.field_names = None;
        }
        if let Some(callable) = other.callable {
            merge_callable(&mut self.callable, callable);
        }
        self.performed.extend(other.performed);
        for (name, value) in other.fields {
            let _ = self
                .fields
                .entry(name)
                .and_modify(|slot| slot.union(value.clone()))
                .or_insert(value);
        }
        if let Some(element) = other.element {
            merge_boxed_value(&mut self.element, *element);
        }
        if let Some(value) = other.result_payload {
            merge_boxed_value(&mut self.result_payload, *value);
        }
        if let Some(value) = other.fiber_payload {
            merge_boxed_value(&mut self.fiber_payload, *value);
        }
        self.channel_sites.extend(other.channel_sites);
        self.deferred.union(other.deferred);
    }
}
