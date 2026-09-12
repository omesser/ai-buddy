//! `ai-buddy-verify` — stone-0 agent/CI verify entry (ADR-0027 / #648).
//!
//! Binary: `cargo run -p ai-buddy-verify -- <doctor|units|overlay|cleanup>`

pub mod cleanup;
pub mod doctor;
pub mod overlay;
pub mod paths;
pub mod proof;
pub mod units;
