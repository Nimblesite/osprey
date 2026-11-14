//! Hindley-Milner type inference for Osprey, together with the built-in
//! function signatures and the match validation (exhaustiveness, auto-wrap)
//! the language requires.
//!
//! The pipeline is the textbook one: a [`ty::Type`] language, an index-addressed
//! union-find substitution ([`ctx::InferCtx`]), [`unify`](unify::unify)
//! with the Osprey-specific rules (`any`, bare-collection generics, structural
//! records, directional Result Success wrapping), let-polymorphism ([`env`]), and a two-pass
//! [`check::check_program`] driver over the AST.
//!
//! Public surface: [`check_program`] takes a parsed [`osprey_ast::Program`] and
//! returns the list of [`TypeError`]s (empty ⇒ well-typed);
//! [`redundant_annotations`] returns the [`TypeWarning`]s for annotations the
//! inferrer would have derived on its own ([TYPE-ANNOTATION-REDUNDANT]).

mod applications;
#[cfg(test)]
mod applications_tests;
mod arithmetic;
mod binders;
mod select;
pub use arithmetic::fold_arithmetic;
mod builtin_constraints;
mod builtin_docs;
mod builtin_docs_core;
mod builtin_docs_gpu;
mod builtin_docs_lang;
mod builtin_docs_native;
mod builtin_docs_signals;
mod builtin_docs_sys;
mod builtins;
mod callback;
#[cfg(test)]
mod callback_tests;
mod check;
mod comparison;
mod convert;
mod ctx;
#[cfg(test)]
mod deferred_arith_tests;
mod effect_rows;
#[cfg(test)]
mod effect_rows_closure_tests;
#[cfg(test)]
mod effect_rows_exports_tests;
#[cfg(test)]
mod effect_rows_expr_tests;
#[cfg(test)]
mod effect_rows_field_tests;
#[cfg(test)]
mod effect_rows_handler_tests;
#[cfg(test)]
mod effect_rows_provenance_tests;
#[cfg(test)]
mod effect_rows_tests;
#[cfg(test)]
mod effect_rows_transport_tests;
mod env;
mod error;
mod expr;
mod fields;
#[cfg(test)]
mod generic_effects_tests;
#[cfg(test)]
mod generics_apply_ml_tests;
#[cfg(test)]
mod generics_apply_tests;
#[cfg(test)]
mod generics_decl_tests;
#[cfg(test)]
mod generics_variance_assign_tests;
#[cfg(test)]
mod generics_variance_builtin_tests;
#[cfg(test)]
mod generics_variance_shape_tests;
#[cfg(test)]
mod generics_variance_tests;
mod handler_scope;
mod info;
mod init_order;
mod methods;
mod multiplicity;
pub mod numeric;
mod pattern;
mod prelude;
pub use prelude::{is_prelude_line, PRELUDE_LINE_BASE};
mod redundant;
#[cfg(test)]
mod redundant_contract_tests;
mod redundant_sites;
mod site_applications;
pub use site_applications::SiteApplications;
#[cfg(test)]
mod redundant_corpus_tests;
#[cfg(test)]
mod redundant_tests;
#[cfg(test)]
mod redundant_window_tests;
#[cfg(test)]
mod site_applications_tests;
mod staging;
mod structural_equals;
#[cfg(test)]
#[path = "../../testkit.rs"]
mod testkit;
#[cfg(test)]
mod testutil;
mod ty;
mod undocumented;
#[cfg(test)]
mod undocumented_tests;
mod unify;
#[cfg(test)]
mod unify_position_tests;
mod unused;
#[cfg(test)]
mod unused_tests;
mod variance;

pub use builtin_constraints::is_gpu_scalar;
pub use builtin_docs::{
    builtin_doc_view, builtin_hover_markdown, builtin_names, BuiltinDocView, BuiltinParam,
};
pub use builtins::{builtin_callback_type, builtin_function_type, builtin_signature};
pub use callback::{redundant_callbacks, REDUNDANT_CALLBACK};
pub use check::{
    check_and_publish, check_program, check_program_exports, effect_check_count, erased_var,
    infer_program, CheckedProgram,
};
pub use effect_rows::LibraryExports;
pub use error::TypeError;
pub use info::{
    CtorLayout, EffectRequirements, HandlerSite, OpType, PerformSite, ProgramTypes, SiteTable,
};
pub use redundant::{
    redundant_annotation_sites, redundant_annotation_sites_for, redundant_annotation_sites_where,
    redundant_annotations, redundant_annotations_where, RedundantAnnotation, RedundantTarget,
    TypeWarning, REDUNDANT_ANNOTATION,
};
pub use staging::{lower_static_checked, signal_dependencies};
pub use ty::{has_type_var, names, render_with_holes, Scheme, Type, VarId, HOLE};
pub use undocumented::{undocumented_exports, UNDOCUMENTED_EXPORT};
pub use unused::{checked_method_reads, unused_symbols, UnusedKind, UnusedSymbol};
#[cfg(test)]
mod tests;
