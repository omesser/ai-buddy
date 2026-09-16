//! `ai-buddy-verify` — agent/CI verify entry (ADR-0027).
//!
//! Binary: `cargo run -p ai-buddy-verify -- <doctor|units|overlay|poke|summon|cleanup>`

pub mod cleanup;
pub mod doctor;
pub mod gesture;
pub mod overlay;
pub mod paths;
pub mod poke;
pub mod proof;
pub mod summon;
pub mod units;
