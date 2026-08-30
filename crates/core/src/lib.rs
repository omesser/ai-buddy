//! The pure core: everything ai-buddy knows how to do without a window server.
//!
//! Nothing here depends on Tauri or on a platform binding, which is a property
//! of the crate rather than a convention — see docs/SPEC.md. Adapters that reach
//! the outside world are declared here as traits and implemented in the shell.

pub mod character;
pub mod director;
pub mod engine;
pub mod input;
pub mod memory;
pub mod overlay;
pub mod roster;
pub mod sensing;
pub mod snapshot;
pub mod tools;
pub mod visibility;
pub mod window_source;
