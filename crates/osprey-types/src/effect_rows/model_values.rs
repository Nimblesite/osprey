//! Effect row model values.
use super::{
    merge_boxed_value, merge_callable, widen_value, BTreeMap, BTreeSet, Callable, EmptyCase,
    Projection, Summary, Value,
};
use std::collections::btree_map::Entry;

impl Value {
    pub(super) fn proves_projection_absent(&self, projection: &Projection) -> bool {
        !self.unknown_structure
            && match projection {
                Projection::Field(field) => {
                    self.empty_case == Some(EmptyCase::Nullary)
                        || self
                            .field_names
                            .as_ref()
                            .is_some_and(|names| !names.contains(field))
                }
                Projection::Element => self.empty_case == Some(EmptyCase::Collection),
                Projection::SuccessValue => self.empty_case == Some(EmptyCase::Error),
                _ => false,
            }
    }

    pub(super) fn union_replacement(&mut self, replacement: Self) {
        if !self.carries_provenance() {
            self.field_names.clone_from(&replacement.field_names);
            self.field_owner.clone_from(&replacement.field_owner);
            self.empty_case = replacement.empty_case;
        }
        self.union(replacement);
    }

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
    let mut actual = unknown_child_record();
    let expected = unknown_child_record();
    let ((), clones) = super::provenance::measured_clones(|| actual.union(unknown_child_record()));
    assert_eq!(actual, expected);
    assert_eq!(clones, 0, "owned fields must move into their merge");
}

#[cfg(test)]
pub(super) fn unknown_child_record() -> Value {
    Value {
        fields: [("child".to_owned(), Value::unknown_value())]
            .into_iter()
            .collect(),
        ..Value::default()
    }
}

#[cfg(test)]
pub(super) fn returning_unknown(index: usize) -> Callable {
    Callable::Known(Box::new(super::KnownCallable {
        parameters: vec![format!("argument{index}")],
        summary: Summary::default(),
        returned: Some(Box::new(Value::unknown_value())),
    }))
}

#[cfg(test)]
pub(super) fn test_summary_metadata() -> Summary {
    Summary {
        required: [super::Requirement::new("Read", "get", Vec::new())]
            .into_iter()
            .collect(),
        runtime_builtins: ["list.map".to_owned()].into_iter().collect(),
        ..Summary::default()
    }
}

#[cfg(test)]
fn known_stack() -> Value {
    Value {
        field_names: Some(["children".to_owned()].into_iter().collect()),
        ..Value::default()
    }
}

#[cfg(test)]
fn known_absence(value: &Value, projection: Projection) -> bool {
    super::identity_tests::with_empty_analyzer(|analyzer| {
        analyzer.projection_absent(value, &[projection])
    })
}

#[cfg(test)]
#[test]
fn known_absence_proves_a_missing_declared_field() {
    assert!(known_absence(
        &known_stack(),
        Projection::Field("edit".to_owned())
    ));
}

#[cfg(test)]
#[test]
fn known_absence_proves_an_empty_collection_element() {
    let value = Value {
        empty_case: Some(EmptyCase::Collection),
        ..Value::default()
    };
    assert!(known_absence(&value, Projection::Element));
}

#[cfg(test)]
#[test]
fn known_absence_preserves_present_fields_without_tracked_payloads() {
    assert!(!known_absence(
        &known_stack(),
        Projection::Field("children".to_owned())
    ));
}

#[cfg(test)]
#[test]
fn known_absence_refuses_opaque_or_unknown_structure() {
    for value in [
        Value::default(),
        Value {
            unknown_structure: true,
            ..known_stack()
        },
        Value::parameter(0, 0),
    ] {
        assert!(!known_absence(&value, Projection::Field("edit".to_owned())));
    }
}

#[cfg(test)]
fn substituted_field(value: Value) -> Value {
    let symbolic = Value::from_callable(Callable::Parameter {
        level: 0,
        index: 0,
        projection: vec![Projection::Field("edit".to_owned())],
    });
    super::identity_tests::with_empty_analyzer(|analyzer| {
        analyzer.substitute_value_at(symbolic, 0, &[Some(value)])
    })
}

#[cfg(test)]
#[test]
fn known_absence_substitution_does_not_invent_an_unknown_callback() {
    assert_eq!(substituted_field(known_stack()), Value::default());
}

#[cfg(test)]
#[test]
fn known_absence_substitution_keeps_opaque_callbacks_closed() {
    for (value, expected) in [
        (Value::default(), Value::unknown_callable()),
        (
            Value {
                unknown_structure: true,
                ..known_stack()
            },
            Value::unknown_value(),
        ),
    ] {
        assert_eq!(substituted_field(value), expected);
    }
}

#[cfg(test)]
#[test]
fn owned_handled_children_do_not_clone_their_payloads() {
    let value = unknown_child_record();
    let expected = value.clone();
    let answers = [(
        super::Requirement::new("Read", "get", Vec::new()),
        Value::default(),
    )]
    .into_iter()
    .collect();
    let (actual, clones) = super::provenance::measured_clones(|| {
        super::Analyzer::resolve_handled_value(value, &answers)
    });
    assert_eq!(actual, expected);
    assert_eq!(clones, 0, "owned handled child clones");
}
