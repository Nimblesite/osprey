//! The Osprey language server.
//!
//! A Rust LSP built on the published `lspkit` crates: [`lspkit_server`] for the
//! JSON-RPC transport/dispatch, [`lspkit_vfs`] for the open-document store, and
//! [`lspkit`]'s `EngineApi` contract (implemented by [`engine::OspreyEngine`])
//! for analysis. The compiler is driven in-process (`osprey_syntax` /
//! `osprey_types`) — diagnostics, outline, hover and navigation are computed
//! from the real AST, not by shelling out. [LSP-REUSE-LSPKIT]

pub mod analysis;
pub mod diagnostics;
pub mod engine;
pub mod features;
pub mod keywords;
pub mod mlrender;
pub mod model;
pub mod server;
pub mod testing;
pub mod text;
pub mod wire;

pub use crate::analysis::{builtin_hover, symbols_json};
pub use crate::engine::OspreyEngine;
pub use crate::server::run_stdio;
pub use crate::testing::tests_json;
