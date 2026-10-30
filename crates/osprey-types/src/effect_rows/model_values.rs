//! Effect row model values.
use super::{
    merge_boxed_value, merge_callable, widen_value, BTreeMap, BTreeSet, Callable, Summary, Value,
};
use std::collections::btree_map::Entry;

impl Value {
    /// Container payloads in field/list/result/fiber order; callable bodies and
    /// deferred summaries require their own scope-aware traversal.
    pub(super) fn container_children_mut(&mut self) -> impl Iterator<Item = &mut Self> {
        self.fields.values_mut().chain(
            [
                self.element.as_deref_mut(),
                self.result_payload.as_deref_mut(),
                self.fiber_payload.as_deref_mut(),
            ]
            .into_iter()
            .flatten(),
        )
    }

    pub(super) fn child_count(&self) -> usize {
        self.fields.len()
            + usize::from(self.element.is_some())
            + usize::from(self.result_payload.is_some())
            + usize::from(self.fiber_payload.is_some())
            + self.callable.as_ref().map_or(0, |callable| match callable {
                Callable::Alternatives(choices) => choices.len(),
                _ => 1,
            })
            + usize::from(!self.deferred.parameter_uses.is_empty())
    }

    pub(super) fn carries_provenance(&self) -> bool {
        self.callable.is_some()
            || self.unknown_structure
            || self.empty_case.is_some()
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

    pub(super) fn unknown_value() -> Self {
        Self {
            unknown_structure: true,
            ..Self::unknown_callable()
        }
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
        self.union_identity(&other);
        if let Some(callable) = other.callable {
            merge_callable(&mut self.callable, callable);
        }
        self.performed.extend(other.performed);
        self.union_fields(other.fields);
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

    fn union_identity(&mut self, other: &Self) {
        self.unknown_structure |= other.unknown_structure;
        if self.field_owner != other.field_owner {
            self.field_owner = None;
        }
        if self.empty_case != other.empty_case {
            self.empty_case = None;
        }
        if self.field_names != other.field_names {
            self.field_names = None;
        }
    }

    fn union_fields(&mut self, fields: BTreeMap<String, Self>) {
        for (name, value) in fields {
            match self.fields.entry(name) {
                Entry::Occupied(mut slot) => slot.get_mut().union(value),
                Entry::Vacant(slot) => {
                    let _ = slot.insert(value);
                }
            }
        }
    }
}

#[cfg(test)]
#[test]
fn owned_record_field_merges_move_subtrees_without_cloning() {
    let record = || Value {
        fields: [("child".to_owned(), Value::unknown_value())]
            .into_iter()
            .collect(),
        ..Value::default()
    };
    let mut actual = record();
    let expected = record();
    let ((), clones) = super::provenance::measured_clones(|| actual.union(record()));
    assert_eq!(actual, expected);
    assert_eq!(clones, 0, "owned fields must move into their merge");
}
