//! First-class `poke` subcommand: a single real click on the sprite, proven
//! by a `verbs:.*Poke` line in the app's own trace (ADR-0027 stone 2, #647).

use std::path::Path;

use crate::gesture::{self, Verb};
use crate::paths::RunPaths;

pub fn run(repo_root: &Path, paths: &RunPaths) -> i32 {
    gesture::run(Verb::Poke, repo_root, paths)
}
