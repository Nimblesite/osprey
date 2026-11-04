//! What a dynamic handler region leaves in the IR, shared by every suite that
//! asserts a static region leaves none of it ([STAGE-RESIDUE]).

/// The C runtime symbols a dynamic handler region registers and looks up.
pub(crate) const HANDLER_RUNTIME_SYMBOLS: &[&str] =
    &["__osprey_handler_push", "__osprey_handler_lookup"];
