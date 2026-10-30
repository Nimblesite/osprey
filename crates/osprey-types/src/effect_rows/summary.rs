//! Effect row summary.
use super::{
    map_parameter_use_values, widen_value_budget, CallArguments, HashSet, ParameterUse, Projection,
    Requirement, Scope, Summary, MASK_LIMIT, MAX_PROVENANCE_DEPTH, MAX_PROVENANCE_NODES,
};
use std::collections::BTreeMap;

impl Requirement {
    /// A requirement keyed by effect identity: the base name plus the
    /// resolved arguments, so the written spelling `Stash<int>` and `Stash`
    /// name one effect. Implements [EFFECTS-GENERIC-INSTANTIATION].
    pub(super) fn new(effect: &str, operation: &str, arguments: Vec<String>) -> Self {
        Self {
            effect: osprey_ast::effect_name::base(effect).to_owned(),
            operation: operation.to_owned(),
            arguments,
            masked: 0,
        }
    }

    /// The same request with no mask over it — the identity a handler answers.
    fn unmasked(&self) -> Self {
        Self {
            masked: 0,
            ..self.clone()
        }
    }

    /// Pass `handlers` answering activations: discharged once they outnumber
    /// the masks over it. A saturated request is never discharged.
    fn passed(self, handlers: u32) -> Option<Self> {
        if self.masked >= MASK_LIMIT {
            return Some(self);
        }
        self.masked
            .checked_sub(handlers)
            .map(|masked| Self { masked, ..self })
    }
}

impl ParameterUse {
    /// Record one more enclosing layer, merging adjacent handlers.
    fn under(&mut self, scope: Scope) {
        match (self.scopes.last_mut(), scope) {
            (Some(Scope::Handled(inner)), Scope::Handled(outer)) => {
                for (requirement, count) in outer {
                    let slot = inner.entry(requirement).or_default();
                    *slot = slot.saturating_add(count).min(MASK_LIMIT);
                }
            }
            (_, scope) => self.scopes.push(scope),
        }
    }
}

/// Whether `open` is `resolved` with some arguments left to inference: the same
/// arity, each argument equal or an inference variable.
fn instantiates(open: &[String], resolved: &[String]) -> bool {
    open.len() == resolved.len()
        && open
            .iter()
            .zip(resolved)
            .all(|(request, answer)| request == answer || is_inference_variable(request))
}

/// An inference variable renders as `t` plus digits (`Type::Var`).
fn is_inference_variable(argument: &str) -> bool {
    argument
        .strip_prefix('t')
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

fn has_inference_variable(arguments: &[String]) -> bool {
    arguments
        .iter()
        .any(|argument| is_inference_variable(argument))
}

/// The exclusion a compile-time handler records for a callback: it answers
/// the operation at any instantiation still left to inference, exactly as
/// [`Summary::without_operations`] does for a lexical request.
const OPEN_INSTANTIATION: &str = "<open>";

fn open(requirement: &Requirement) -> Requirement {
    Requirement {
        arguments: vec![OPEN_INSTANTIATION.to_owned()],
        ..requirement.clone()
    }
}

impl CallArguments {
    /// Function-value slots have no parameter names. Do this at the call site,
    /// before substitution reveals a callback's declaration and its names.
    pub(super) fn in_written_order(mut self) -> Self {
        self.positional
            .extend(self.named.drain(..).map(|(_, value)| value));
        self
    }
}

impl Summary {
    // Union monotone requirements, but re-derive the dynamic verdict each pass.
    // Converge supplies an equal immutable row; a different caller retains its
    // own snapshot so changed flags never depend on an unrelated row.
    pub(super) fn merge_inferred_row(&mut self, actual: Self, previous: Option<&Self>) -> bool {
        let before = match previous.filter(|row| *row == self) {
            Some(row) => std::borrow::Cow::Borrowed(row),
            None => std::borrow::Cow::Owned(self.clone()),
        };
        let resolved = actual.unresolved_dynamic_call;
        self.union(actual);
        self.unresolved_dynamic_call = resolved;
        self.widen();
        *self != *before
    }

    pub(super) fn union(&mut self, other: Self) {
        self.required.extend(other.required);
        self.runtime_builtins.extend(other.runtime_builtins);
        self.parameter_uses.extend(other.parameter_uses);
        self.unresolved_dynamic_call |= other.unresolved_dynamic_call;
    }

    /// Remove the operations a handler at `arguments` discharges. A static
    /// handler also answers a request whose instantiation is still an
    /// inference variable: discharge rewrites that request into the arm body,
    /// and the residual program's type check then proves the instantiation
    /// agreed ([EFFECTS-GENERIC-INSTANTIATION], [STAGE-LOWER-ORDER-PHASE]). A
    /// runtime handler needs the resolved identity ([EFFECTS-GENERIC-RUNTIME]).
    pub(super) fn without_operations(
        self,
        effect: &str,
        arguments: &[String],
        operations: &HashSet<String>,
        compile_time: bool,
    ) -> Self {
        let answers = |r: &Requirement| {
            u32::from(
                r.effect == effect
                    && operations.contains(&r.operation)
                    && (r.arguments == arguments
                        || (compile_time && instantiates(&r.arguments, arguments))),
            )
        };
        let summary = self.discharged(answers);
        if operations.is_empty() {
            return summary;
        }
        // A callback requirement is not concrete until its call site. Record
        // every operation this handler can discharge.
        let answered = operations.iter().flat_map(|operation| {
            let answered = Requirement::new(effect, operation, arguments.to_vec());
            compile_time
                .then(|| open(&answered))
                .into_iter()
                .chain(std::iter::once(answered))
        });
        summary.under(&Scope::Handled(answered.map(|r| (r, 1)).collect()))
    }

    /// Pass every requirement `answers` matches through one answering
    /// activation: it is discharged unless a mask still skips it.
    fn discharged(mut self, answers: impl Fn(&Requirement) -> u32) -> Self {
        self.required = std::mem::take(&mut self.required)
            .into_iter()
            .filter_map(|r| match answers(&r) {
                0 => Some(r),
                handlers => r.passed(handlers),
            })
            .collect();
        self
    }

    /// `mask` over this summary: each request of the masked identity skips one
    /// more answering activation ([EFFECTS-MASK]).
    pub(super) fn masked(mut self, effect: &str, arguments: &[String]) -> Self {
        self.required = std::mem::take(&mut self.required)
            .into_iter()
            .map(|mut r| {
                if r.effect == effect && r.arguments == arguments {
                    r.masked = (r.masked + 1).min(MASK_LIMIT);
                }
                r
            })
            .collect();
        let scope = Scope::Masked {
            effect: effect.to_owned(),
            arguments: arguments.to_vec(),
        };
        self.under(&scope)
    }

    /// Record `scope` as one more layer over every symbolic callback use.
    fn under(mut self, scope: &Scope) -> Self {
        self.parameter_uses = std::mem::take(&mut self.parameter_uses)
            .into_iter()
            .map(|mut use_| {
                use_.under(scope.clone());
                use_
            })
            .collect();
        self
    }

    /// Apply a callback use's recorded layers, innermost first, to the
    /// callback's own summary.
    pub(super) fn within(self, scopes: &[Scope]) -> Self {
        scopes.iter().fold(self, |summary, scope| match scope {
            Scope::Handled(excluded) => summary.excluding(excluded),
            Scope::Masked { effect, arguments } => summary.masked(effect, arguments),
        })
    }

    fn excluding(self, excluded: &BTreeMap<Requirement, u32>) -> Self {
        let answers = |r: &Requirement| {
            let request = r.unmasked();
            let opened = has_inference_variable(&r.arguments)
                .then(|| excluded.get(&open(&request)).copied())
                .flatten();
            excluded.get(&request).copied().max(opened).unwrap_or(0)
        };
        self.discharged(answers)
            .under(&Scope::Handled(excluded.clone()))
    }

    pub(super) fn widen(&mut self) {
        self.widen_at(0);
    }

    pub(super) fn widen_at(&mut self, depth: usize) {
        self.widen_at_budget(depth, MAX_PROVENANCE_NODES);
    }

    pub(super) fn widen_at_budget(&mut self, depth: usize, budget: usize) {
        let before = self.parameter_uses.len();
        self.parameter_uses
            .retain(|use_| bounded_parameter_use(use_, depth));
        if self.parameter_uses.len() != before {
            self.unresolved_dynamic_call = true;
        }
        self.parameter_uses = std::mem::take(&mut self.parameter_uses)
            .into_iter()
            .map(|mut use_| {
                map_parameter_use_values(&mut use_, |value| {
                    *value = widen_value_budget(std::mem::take(value), depth + 1, budget);
                });
                use_
            })
            .collect();
    }
}

pub(super) fn bounded_parameter_use(use_: &ParameterUse, depth: usize) -> bool {
    use_.level < MAX_PROVENANCE_DEPTH
        && use_.projection.len() < MAX_PROVENANCE_DEPTH
        && (depth < MAX_PROVENANCE_DEPTH
            || (use_.arguments.positional.is_empty()
                && use_.arguments.named.is_empty()
                && !use_.projection.iter().any(|part| {
                    matches!(
                        part,
                        Projection::Method(_) | Projection::Returned(_) | Projection::Handled(_)
                    )
                })))
}

#[cfg(test)]
#[test]
fn summary_widen_moves_owned_children_without_cloning() {
    let mut actual = widening_fixture();
    let expected = actual.clone();
    let ((), clones) = super::provenance::measured_clones(|| actual.widen());
    assert_eq!(actual, expected);
    assert_eq!(
        clones, 0,
        "owned summary children must move through widening"
    );
}

#[cfg(test)]
fn widening_arguments() -> CallArguments {
    CallArguments {
        positional: vec![Some(widening_child()), None],
        named: vec![
            ("given".to_owned(), Some(widening_child())),
            ("missing".to_owned(), None),
        ],
    }
}

#[cfg(test)]
fn widening_projections() -> Vec<Projection> {
    vec![
        widening_method(),
        Projection::Returned(Box::new(widening_arguments())),
        Projection::Handled(
            [(
                Requirement::new("Read", "get", Vec::new()),
                widening_child(),
            )]
            .into_iter()
            .collect(),
        ),
    ]
}

#[cfg(test)]
fn widening_fixture() -> Summary {
    Summary {
        parameter_uses: [ParameterUse {
            level: 1,
            index: 2,
            projection: widening_projections(),
            arguments: widening_arguments(),
            scopes: Vec::new(),
        }]
        .into_iter()
        .collect(),
        unresolved_dynamic_call: false,
        ..super::model_values::test_summary_metadata()
    }
}

#[cfg(test)]
#[test]
fn summary_widen_depth_limit_still_refuses_nested_invocations() {
    let mut actual = widening_fixture();
    let mut expected = actual.clone();
    expected.parameter_uses.clear();
    expected.unresolved_dynamic_call = true;
    actual.widen_at(MAX_PROVENANCE_DEPTH);
    assert_eq!(actual, expected);
}

#[cfg(test)]
fn widening_child() -> super::Value {
    let mut value = super::model_values::unknown_child_record();
    if let Some(child) = value.fields.get_mut("child") {
        child.deferred.unresolved_dynamic_call = true;
    }
    value
}

#[cfg(test)]
fn widening_method() -> Projection {
    let arguments = widening_arguments();
    Projection::Method(Box::new(super::MethodProjection {
        field: "callback".to_owned(),
        arguments: arguments.positional,
        named: arguments.named,
        fallback: widening_child(),
    }))
}

#[cfg(test)]
#[test]
fn inferred_row_mismatched_snapshot_keeps_its_own_changed_flag() {
    let mut row = super::model_values::test_summary_metadata();
    let expected = row.clone();
    assert!(!row.merge_inferred_row(Summary::default(), Some(&Summary::default())));
    assert_eq!(row, expected);
}
