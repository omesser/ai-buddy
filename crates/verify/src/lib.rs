//! `ai-buddy-verify` — agent/CI verify entry (ADR-0027).
//!
//! Binary: `cargo run -p ai-buddy-verify -- <doctor|units|overlay|poke|summon|cleanup>`
//!
//! What a caller may depend on — exit codes, `--json`, the `PROOF.md` section —
//! is [`contract`].

pub mod cleanup;
pub mod contract;
pub mod doctor;
pub mod gesture;
pub mod overlay;
pub mod paths;
pub mod poke;
pub mod proof;
pub mod summon;
pub mod units;
