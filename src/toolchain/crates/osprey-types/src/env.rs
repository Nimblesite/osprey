//! The typing environment and the two operations that make HM polymorphic:
//! `instantiate` (fresh-rename a scheme's bound vars at each use) and
//! `generalize` (quantify a let-binding over the vars not free in the
//! environment).

use crate::ctx::InferCtx;
use crate::ty::{Scheme, Type, VarId};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

mod builtin_bindings;

/// One call's signature, deferred obligations, and ordered declared binders.
pub(crate) struct AppliedSignature {
    pub ty: Type,
    pub obligations: Vec<(String, Type)>,
    pub params: Vec<Type>,
    pub bindings: HashMap<VarId, Type>,
}

/// One scope's own bindings. A `None` or `false` entry records that this scope
/// removed or shadowed a name, so a lookup stops there instead of reaching an
/// outer scope's binding.
#[derive(Debug, Clone, Default)]
struct Frame {
    vars: HashMap<String, Option<Arc<Scheme>>>,
    /// The subset of `vars` whose scheme may name a variable it does not itself
    /// quantify ([`possibly_open`]) — the only bindings [`TypeEnv::free_vars`]
    /// has to walk. Every other binding is closed and can contribute nothing.
    open: HashMap<String, Arc<OpenScheme>>,
    /// The names bound here that hide a same-named, possibly-open binding
    /// further out, so `free_vars` skips the hidden one.
    shadows_open: HashSet<String>,
    /// Ordered declaration binders, cleared whenever a value shadows a name.
    type_params: HashMap<String, Option<Vec<Type>>>,
    /// Names declared `mut` — the only bindings handler-arm assignment may target.
    mutables: HashMap<String, bool>,
    /// Builtin identity and runtime obligations, cleared by shadowing.
    builtins: HashMap<String, Option<builtin_bindings::Builtin>>,
}

/// Cache only syntactic roots. Their substitutions stay live in `InferCtx`.
#[derive(Debug)]
struct OpenScheme {
    scheme: Arc<Scheme>,
    roots: BTreeSet<VarId>,
}

/// Maps names to their type schemes, with value semantics: bindings added to a
/// child scope (lambda bodies, match arms) never leak out, and a scope never
/// sees what its parent binds after the child was made.
///
/// A scope shares its parent instead of copying it: inference opens a child
/// for every lambda, arm and block, and copying an environment holding every
/// builtin and top-level scheme made each one cost the whole environment.
/// A scope writes only its own frame.
#[derive(Debug, Clone, Default)]
pub(crate) struct TypeEnv {
    local: Arc<Frame>,
    outer: Option<Arc<TypeEnv>>,
    depth: usize,
}

/// Past this many nested scopes a child is flattened, so lookups stay short
/// however a pass chains them.
const MAX_DEPTH: usize = 64;

impl TypeEnv {
    pub(crate) fn new() -> TypeEnv {
        TypeEnv::default()
    }

    /// Every frame, innermost first.
    fn frames(&self) -> impl Iterator<Item = &Frame> {
        std::iter::successors(Some(self), |env| env.outer.as_deref()).map(|env| &*env.local)
    }

    /// The innermost frame's entry for a name, whatever it records.
    fn lookup<'a, T>(&'a self, pick: impl Fn(&'a Frame) -> Option<&'a T>) -> Option<&'a T> {
        self.frames().find_map(pick)
    }

    /// This scope's own frame, for writing. A frame another scope still shares
    /// is not copied: this scope becomes a child of its own snapshot instead,
    /// so the write costs one empty frame however large the shared one is. A
    /// scope that many such writes have deepened is flattened once, here, so
    /// the scopes opened from it stay shallow.
    fn frame(&mut self) -> &mut Frame {
        if Arc::get_mut(&mut self.local).is_none() {
            *self = if self.depth < MAX_DEPTH {
                self.child()
            } else {
                self.flattened()
            };
        }
        Arc::make_mut(&mut self.local)
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Scheme> {
        self.lookup(|frame| frame.vars.get(name))
            .and_then(Option::as_deref)
    }

    fn type_params(&self, name: &str) -> Option<&Vec<Type>> {
        self.lookup(|frame| frame.type_params.get(name))
            .and_then(Option::as_ref)
    }

    pub(crate) fn insert(&mut self, name: impl Into<String>, scheme: Scheme) {
        let name = name.into();
        // A fresh binding shadows any outer `mut` of the same name.
        if self.is_mutable(&name) {
            let _ = self.frame().mutables.insert(name.clone(), false);
        }
        self.shadow_declaration(&name);
        self.bind(name, scheme);
    }

    /// Bind a `mut` declaration — the one binding form handler arms may assign.
    pub(crate) fn insert_mutable(&mut self, name: impl Into<String>, scheme: Scheme) {
        let name = name.into();
        self.shadow_declaration(&name);
        self.bind(name.clone(), scheme);
        let _ = self.frame().mutables.insert(name, true);
    }

    /// Record a binding in this scope, along with the two facts `free_vars`
    /// needs to skip it: whether it is possibly open, and whether it hides a
    /// possibly-open binding further out.
    fn bind(&mut self, name: String, scheme: Scheme) {
        let roots = unquantified_vars(&scheme);
        let open = !roots.is_empty();
        #[cfg(test)]
        let open = open || is_witness(&scheme);
        // Over-approximating is harmless: this entry only suppresses a
        // same-named binding further out, and suppressing a binding that is
        // not possibly open takes nothing away.
        let hides_outer = self.frames().any(|frame| frame.open.contains_key(&name));
        let scheme = Arc::new(scheme);
        let frame = self.frame();
        let _ = frame.vars.insert(name.clone(), Some(Arc::clone(&scheme)));
        if open {
            let _ = frame
                .open
                .insert(name.clone(), Arc::new(OpenScheme { scheme, roots }));
        } else {
            let _ = frame.open.remove(&name);
        }
        if hides_outer {
            let _ = frame.shadows_open.insert(name);
        }
    }

    pub(crate) fn is_mutable(&self, name: &str) -> bool {
        self.lookup(|frame| frame.mutables.get(name))
            .is_some_and(|mutable| *mutable)
    }

    /// The currently bound names. Snapshotted on the freshly built builtin
    /// environment to detect redefinition of built-in functions.
    pub(crate) fn bound_names(&self) -> HashSet<String> {
        let mut decided = HashSet::new();
        let mut bound = HashSet::new();
        for frame in self.frames() {
            for (name, scheme) in &frame.vars {
                if decided.insert(name.as_str()) && scheme.is_some() {
                    let _ = bound.insert(name.clone());
                }
            }
        }
        bound
    }

    pub(crate) fn remove(&mut self, name: &str) {
        self.shadow_declaration(name);
        if self.get(name).is_some() {
            // Those frames' open entries are what the snapshot below consults,
            // so ask before the scope is reopened for writing.
            let hides_outer = self.frames().any(|frame| frame.open.contains_key(name));
            let frame = self.frame();
            let _ = frame.vars.insert(name.to_owned(), None);
            let _ = frame.open.remove(name);
            if hides_outer {
                let _ = frame.shadows_open.insert(name.to_owned());
            }
        }
    }

    /// Attach declaration-site binders to this exact lexical binding.
    pub(crate) fn declare_type_params(&mut self, name: &str, params: Vec<Type>) {
        let _ = self
            .frame()
            .type_params
            .insert(name.to_owned(), Some(params));
    }

    /// The declaration-site binders exactly as declared — NOT re-instantiated
    /// against the current scheme. A body check quantifies the very variables
    /// the signature exported, which a substituted copy would orphan.
    pub(crate) fn declared_params(&self, name: &str) -> Vec<Type> {
        self.type_params(name).cloned().unwrap_or_default()
    }

    /// Instantiate the signature and its declared binders with one substitution.
    pub(crate) fn applied(&self, ctx: &mut InferCtx, name: &str) -> Option<AppliedSignature> {
        let scheme = self.get(name)?;
        let map = scheme.vars.iter().map(|v| (*v, ctx.fresh())).collect();
        let params = self
            .type_params(name)
            .into_iter()
            .flatten()
            .map(|ty| subst_vars(&ctx.apply(ty), &map))
            .collect();
        let obligations = subst_obligations(&scheme.obligations, &map);
        Some(AppliedSignature {
            ty: subst_vars(&scheme.ty, &map),
            obligations,
            params,
            bindings: map,
        })
    }

    /// A fresh child scope: bindings added to the child don't leak.
    pub(crate) fn child(&self) -> TypeEnv {
        let outer = if self.depth < MAX_DEPTH {
            self.clone()
        } else {
            self.flattened()
        };
        TypeEnv {
            local: Arc::default(),
            depth: outer.depth + 1,
            outer: Some(Arc::new(outer)),
        }
    }

    /// Shorten this chain while still showing exactly what this scope sees:
    /// the frames holding the fewest bindings are merged, and the rest stay in
    /// place as a chain.
    ///
    /// A pass copies every entry of every frame it merges, and the frames that
    /// carry the builtins and the program's top-level schemes are the ones a
    /// growing program keeps re-flattening. Leaving those alone keeps each pass
    /// proportional to the small nested frames instead of to the part of the
    /// program already checked.
    fn flattened(&self) -> TypeEnv {
        #[cfg(test)]
        tests::FLATTEN_PASSES.with(|count| count.set(count.get() + 1));
        let sizes: Vec<usize> = self.frames().map(|frame| frame.vars.len()).collect();
        let cut = kept_from(&sizes);
        let mut merged = Frame::default();
        for frame in self.frames().take(cut) {
            merge_into(&mut merged, frame);
        }
        let outer = self.ancestor(cut);
        TypeEnv {
            local: Arc::new(merged),
            depth: outer.as_ref().map_or(0, |env| env.depth + 1),
            outer,
        }
    }

    /// The environment holding every frame from `cut` outwards, sharing them
    /// rather than copying. `cut` is counted from this scope, whose own frame
    /// is 0.
    fn ancestor(&self, cut: usize) -> Option<Arc<TypeEnv>> {
        let mut node = self.outer.clone()?;
        for _ in 1..cut {
            let next = node.outer.clone()?;
            node = next;
        }
        Some(node)
    }

    /// The free variables of the whole environment — the vars `generalize` must
    /// *not* quantify, because an outer scope may still constrain them. A
    /// binding an inner scope shadows or removed is not visible, so it does
    /// not count.
    pub(crate) fn free_vars(&self, ctx: &mut InferCtx) -> BTreeSet<VarId> {
        let mut out = BTreeSet::new();
        let mut decided: HashSet<&str> = HashSet::new();
        for frame in self.frames() {
            for (name, scheme) in &frame.open {
                if !decided.is_empty() && decided.contains(name.as_str()) {
                    continue;
                }
                scheme_free_vars(ctx, scheme, &mut out);
            }
            // Everything this frame hides stays hidden for the frames outside
            // it — including names it binds to a closed scheme, which would
            // otherwise let an outer open binding of the same name through.
            decided.extend(frame.shadows_open.iter().map(String::as_str));
        }
        out
    }

    /// A trial may leave this scope before later constraints run, so even a
    /// shadowed binding in an enclosing frame can still share a variable.
    pub(crate) fn shares_open_vars(&self, ctx: &InferCtx, vars: &BTreeSet<VarId>) -> bool {
        let mut open = BTreeSet::new();
        self.frames().any(|frame| {
            for scheme in frame.open.values() {
                scheme_free_vars(ctx, scheme, &mut open);
            }
            for binder in frame.type_params.values().flatten().flatten() {
                ctx.free_vars(binder, &mut open);
            }
            !open.is_disjoint(vars)
        })
    }
}

/// Whether `scheme` can name a variable it does not itself quantify — the only
/// kind of binding that can contribute to [`TypeEnv::free_vars`].
///
/// This is a syntactic test (it does not resolve through the substitution)
/// because insertion has no [`InferCtx`]. It is still exact: a scheme's
/// quantified variables are never bound by the substitution — `generalize`
/// quantifies only variables `ctx` reports free and that no visible binding
/// shares, and every use of a scheme re-states its body against fresh ones
/// ([`instantiated`], [`TypeEnv::applied`]) instead of unifying the scheme's
/// own body, which is only ever cloned for display and code generation. So a
/// resolved walk of a scheme with no unquantified variable is empty.
pub(crate) fn possibly_open(scheme: &Scheme) -> bool {
    #[cfg(test)]
    if is_witness(scheme) {
        return true;
    }
    !unquantified_vars(scheme).is_empty()
}

/// Tests observe environment scans through a witness binding whose type is
/// named by [`tests::ENVIRONMENT_MARKER`]. Treat it as possibly open so the
/// scan they measure still walks it; no production type carries that name.
#[cfg(test)]
fn is_witness(scheme: &Scheme) -> bool {
    matches!(&scheme.ty, Type::Record { name, .. } | Type::Con { name, .. }
        if name == tests::ENVIRONMENT_MARKER)
}

fn unquantified_vars(scheme: &Scheme) -> BTreeSet<VarId> {
    fn collect(ty: &Type, out: &mut BTreeSet<VarId>) {
        match ty {
            Type::Var(var) => {
                let _ = out.insert(*var);
            }
            Type::Fun { params, ret } => {
                for param in params {
                    collect(param, out);
                }
                collect(ret, out);
            }
            Type::Con { args, .. } => args.iter().for_each(|ty| collect(ty, out)),
            Type::Record { fields, .. } => fields.values().for_each(|ty| collect(ty, out)),
            Type::Union { variants, .. } => variants.iter().for_each(|ty| collect(ty, out)),
        }
    }
    let mut roots = BTreeSet::new();
    collect(&scheme.ty, &mut roots);
    for (_, obligation) in &scheme.obligations {
        collect(obligation, &mut roots);
    }
    roots.retain(|var| !scheme.vars.contains(var));
    roots
}

/// A scheme's free variables: those of its type and obligations it does not
/// quantify.
fn scheme_free_vars(ctx: &InferCtx, open: &OpenScheme, out: &mut BTreeSet<VarId>) {
    let scheme = &open.scheme;
    #[cfg(test)]
    tests::record_environment_visit(scheme);
    let mut scratch = BTreeSet::new();
    let free = if scheme.vars.is_empty() {
        &mut *out
    } else {
        #[cfg(test)]
        tests::SCHEME_SCRATCH.with(|count| count.set(count.get() + 1));
        &mut scratch
    };
    for root in &open.roots {
        ctx.free_vars(&Type::Var(*root), free);
    }
    for q in &scheme.vars {
        let _ = free.remove(q);
    }
    out.extend(scratch);
}

/// A flatten pass keeps at most this many frames in place: the chain has to
/// stay short, and merging everything above a few large frames is what makes a
/// pass cheap.
const FLATTEN_KEEP_MAX: usize = 4;

/// Where a flatten pass stops merging: the outermost frames that together hold
/// at least half the bindings stay as they are, so a chain of many tiny scopes
/// is merged in one pass but the frames carrying the whole program are not
/// copied on every pass.
fn kept_from(sizes: &[usize]) -> usize {
    let total: usize = sizes.iter().sum();
    let mut kept = 0;
    let mut cut = 1;
    for (index, size) in sizes.iter().enumerate().rev() {
        kept += size;
        if kept * 2 >= total {
            cut = index;
            break;
        }
    }
    cut.max(1).max(sizes.len().saturating_sub(FLATTEN_KEEP_MAX))
}

/// Keep the first visible binding, including an explicit shadowing marker.
fn merge_missing<T: Clone>(target: &mut HashMap<String, T>, source: &HashMap<String, T>) {
    for (name, value) in source {
        let _ = target.entry(name.clone()).or_insert_with(|| value.clone());
    }
}

/// Add every binding of `source` that `target` does not already show, so the
/// merged frame shows exactly what the frames it replaces showed.
fn merge_into(target: &mut Frame, source: &Frame) {
    // Before `vars` grows: a name an inner frame already bound or removed hides this one.
    for (name, scheme) in &source.open {
        if !target.vars.contains_key(name) {
            let _ = target.open.insert(name.clone(), Arc::clone(scheme));
        }
    }
    merge_missing(&mut target.vars, &source.vars);
    merge_missing(&mut target.type_params, &source.type_params);
    merge_missing(&mut target.mutables, &source.mutables);
    merge_missing(&mut target.builtins, &source.builtins);
    target
        .shadows_open
        .extend(source.shadows_open.iter().cloned());
}

/// A scheme's built-in obligations restated against the substitution `map` —
/// the same rewrite every instantiation site performs on its fresh variables.
fn subst_obligations(
    obligations: &[(String, Type)],
    map: &HashMap<VarId, Type>,
) -> Vec<(String, Type)> {
    obligations
        .iter()
        .map(|(name, ty)| (name.clone(), subst_vars(ty, map)))
        .collect()
}

/// Instantiate a scheme: replace each quantified variable with a fresh one.
pub(crate) fn instantiate(ctx: &mut InferCtx, scheme: &Scheme) -> Type {
    instantiated(ctx, scheme).0
}

/// [`instantiate`], also re-stating the scheme's built-in obligations against
/// this site's fresh variables. The caller records them so the site is checked
/// against the concrete types it supplies — a generalized wrapper cannot
/// launder an ineligible element type past a constrained built-in
/// ([`Scheme::obligations`]).
pub(crate) fn instantiated(ctx: &mut InferCtx, scheme: &Scheme) -> (Type, Vec<(String, Type)>) {
    if scheme.vars.is_empty() {
        return (scheme.ty.clone(), scheme.obligations.clone());
    }
    let map: HashMap<VarId, Type> = scheme.vars.iter().map(|v| (*v, ctx.fresh())).collect();
    let obligations = subst_obligations(&scheme.obligations, &map);
    (subst_vars(&scheme.ty, &map), obligations)
}

/// Generalize a type and retain the visible environment variables for its obligations.
pub(crate) fn generalize(
    ctx: &mut InferCtx,
    env: &TypeEnv,
    ty: &Type,
) -> (Scheme, BTreeSet<VarId>) {
    let ty_fv = ctx.free_vars_after_defaulting(ty);
    let ty = ctx.apply(ty);
    let external = if ty_fv.is_empty() {
        BTreeSet::new()
    } else {
        env.free_vars(ctx)
    };
    let vars = ty_fv.difference(&external).copied().collect();
    let scheme = Scheme {
        vars,
        ty,
        obligations: Vec::new(),
    };
    (scheme, external)
}

pub(crate) fn subst_vars(t: &Type, map: &HashMap<VarId, Type>) -> Type {
    crate::ty::map_type_vars(t, &mut |v| map.get(&v).cloned().unwrap_or(Type::Var(v)))
}
#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
pub(crate) mod tests;
