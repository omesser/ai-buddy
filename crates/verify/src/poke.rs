//! First-class `poke` subcommand: a single real click on the sprite, proven
//! by a `verbs:.*Poke` line in the app's own trace (ADR-0027 stone 2, #647).

use std::path::Path;

use crate::contract::RunReport;
use crate::gesture::{self, Verb};

pub fn run(repo_root: &Path, report: &mut RunReport) {
    gesture::run(Verb::Poke, repo_root, report)
}
