//! Explicit call substitutions by source site, shared between a program's types
//! and every specialized copy of them. Implements [TYPE-GENERICS-APPLY].
//!
//! Lowering a generic call specializes the program's types against that call's
//! bindings, and every application site's bindings must see them too. Copying
//! and rewriting every site's map on each call made a program's lowering grow
//! with its call sites times its generic calls. The sites stay shared instead,
//! and each specialization's bindings are queued and applied, in order, to the
//! one site a lookup reads.

use crate::ty::{Type, VarId};
use std::collections::HashMap;
use std::sync::Arc;

/// One site's bindings: a type variable → the type the call applies to it.
pub(crate) type Bindings = HashMap<VarId, Type>;

/// Every application site's bindings, with the specializations still to apply.
#[derive(Debug, Clone, Default)]
pub struct SiteApplications {
    sites: Arc<HashMap<(u32, u32), Bindings>>,
    pending: Vec<Arc<Bindings>>,
}

impl SiteApplications {
    /// The bindings at `site`, with every specialization applied in order.
    #[must_use]
    pub fn get(&self, site: &(u32, u32)) -> Option<Bindings> {
        self.sites.get(site).map(|bindings| {
            self.pending
                .iter()
                .fold(bindings.clone(), |current, applied| {
                    current
                        .into_iter()
                        .map(|(var, ty)| (var, crate::env::subst_vars(&ty, applied)))
                        .collect()
                })
        })
    }

    /// Records a site's bindings. Elaboration does this before any
    /// specialization, so an inserted site is read under the same queue as the rest.
    pub(crate) fn insert(&mut self, site: (u32, u32), bindings: Bindings) {
        let _ = Arc::make_mut(&mut self.sites).insert(site, bindings);
    }

    /// The same sites, read under `bindings` after every earlier specialization.
    #[must_use]
    pub(crate) fn specialized(&self, bindings: &Bindings) -> Self {
        let mut pending = self.pending.clone();
        pending.push(Arc::new(bindings.clone()));
        Self {
            sites: Arc::clone(&self.sites),
            pending,
        }
    }
}

impl FromIterator<((u32, u32), Bindings)> for SiteApplications {
    fn from_iter<I: IntoIterator<Item = ((u32, u32), Bindings)>>(sites: I) -> Self {
        Self {
            sites: Arc::new(sites.into_iter().collect()),
            pending: Vec::new(),
        }
    }
}
