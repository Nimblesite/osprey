//! Tables a generic call specializes without copying them.
//!
//! The backend specializes the published types once per generic call. A copy of
//! every table at each call made the build time of a program the square of its
//! size, so a specialization shares each table and holds only the entries it
//! instantiated. Implements [TYPE-GENERICS-APPLY].

use crate::ty::{Type, VarId};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) type Bindings = HashMap<VarId, Type>;

/// A table value whose types a generic call may instantiate.
pub trait Typed: Clone {
    /// Every type the value holds.
    fn types(&self) -> impl Iterator<Item = &Type>;
    /// The same types, to instantiate in place.
    fn types_mut(&mut self) -> impl Iterator<Item = &mut Type>;
}

impl Typed for (Vec<Type>, Type) {
    fn types(&self) -> impl Iterator<Item = &Type> {
        self.0.iter().chain(std::iter::once(&self.1))
    }

    fn types_mut(&mut self) -> impl Iterator<Item = &mut Type> {
        self.0.iter_mut().chain(std::iter::once(&mut self.1))
    }
}

impl<S: std::hash::BuildHasher + Clone> Typed for HashMap<String, Type, S> {
    fn types(&self) -> impl Iterator<Item = &Type> {
        self.values()
    }

    fn types_mut(&mut self) -> impl Iterator<Item = &mut Type> {
        self.values_mut()
    }
}

impl Typed for super::OpType {
    fn types(&self) -> impl Iterator<Item = &Type> {
        self.params.iter().chain(std::iter::once(&self.ret))
    }

    fn types_mut(&mut self) -> impl Iterator<Item = &mut Type> {
        self.params.iter_mut().chain(std::iter::once(&mut self.ret))
    }
}

impl Typed for super::PerformSite {
    fn types(&self) -> impl Iterator<Item = &Type> {
        self.effect_args.iter().chain(self.op.types())
    }

    fn types_mut(&mut self) -> impl Iterator<Item = &mut Type> {
        self.effect_args.iter_mut().chain(self.op.types_mut())
    }
}

impl Typed for super::HandlerSite {
    fn types(&self) -> impl Iterator<Item = &Type> {
        let operations = self.ops.values().flat_map(Typed::types);
        self.effect_args.iter().chain(operations)
    }

    fn types_mut(&mut self) -> impl Iterator<Item = &mut Type> {
        let operations = self.ops.values_mut().flat_map(Typed::types_mut);
        self.effect_args.iter_mut().chain(operations)
    }
}

/// Whether `bindings` instantiates a variable that `value` mentions.
fn touched(value: &impl Typed, bindings: &Bindings) -> bool {
    let bound = |var| bindings.contains_key(&var);
    value.types().any(|ty| crate::ty::any_type_var(ty, &bound))
}

fn instantiated<V: Typed>(value: &V, bindings: &Bindings) -> V {
    let mut value = value.clone();
    for ty in value.types_mut() {
        *ty = crate::env::subst_vars(ty, bindings);
    }
    value
}

/// Instantiate the sites `bindings` touches; an untouched table stays shared.
pub(crate) fn specialize_sites<V: Typed>(
    sites: &mut Arc<HashMap<(u32, u32), V>>,
    bindings: &Bindings,
) {
    if sites.values().any(|site| touched(site, bindings)) {
        for site in Arc::make_mut(sites).values_mut() {
            *site = instantiated(site, bindings);
        }
    }
}

/// Values by name: one table shared by every specialization, and the few
/// entries each specialization instantiated.
#[derive(Debug, Clone)]
pub struct Named<V> {
    shared: Arc<HashMap<String, V>>,
    /// The names whose shared value mentions a type variable: all a call can instantiate.
    open: Arc<Vec<String>>,
    instantiated: HashMap<String, V>,
}

impl<V> Default for Named<V> {
    fn default() -> Self {
        Self {
            shared: Arc::default(),
            open: Arc::default(),
            instantiated: HashMap::new(),
        }
    }
}

impl<V> Named<V> {
    /// The value under `name`, as this specialization instantiated it.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&V> {
        self.instantiated
            .get(name)
            .or_else(|| self.shared.get(name))
    }

    /// Whether a value is recorded under `name`.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.shared.contains_key(name)
    }

    /// Every name with its value, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &V)> {
        self.shared
            .iter()
            .map(|(name, value)| (name, self.instantiated.get(name).unwrap_or(value)))
    }
}

impl<'a, V> IntoIterator for &'a Named<V> {
    type Item = (&'a String, &'a V);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl<V: Typed> From<HashMap<String, V>> for Named<V> {
    fn from(shared: HashMap<String, V>) -> Self {
        let open = shared
            .iter()
            .filter(|(_, value)| value.types().any(crate::has_type_var))
            .map(|(name, _)| name.clone())
            .collect();
        Self {
            shared: Arc::new(shared),
            open: Arc::new(open),
            instantiated: HashMap::new(),
        }
    }
}

impl<V: Typed> Named<V> {
    /// Instantiate `function`'s value, or every value when no function is named.
    pub(crate) fn specialize(&mut self, function: Option<&str>, bindings: &Bindings) {
        match function {
            Some(name) => self.instantiate(name, bindings),
            None => {
                for name in Arc::clone(&self.open).iter() {
                    self.instantiate(name, bindings);
                }
            }
        }
    }

    fn instantiate(&mut self, name: &str, bindings: &Bindings) {
        let value = self
            .get(name)
            .filter(|value| touched(*value, bindings))
            .map(|value| instantiated(value, bindings));
        if let Some(value) = value {
            let _ = self.instantiated.insert(name.to_owned(), value);
        }
    }
}
