//! The four export writers [PROF-CLI-RUN]: speedscope (primary), V8
//! `.cpuprofile` (VS Code's built-in viewer), Brendan Gregg collapsed stacks
//! (flame-graph tooling), and the editor-integration summary.

pub(crate) mod cpuprofile;
pub(crate) mod folded;
pub(crate) mod speedscope;
pub(crate) mod summary;
