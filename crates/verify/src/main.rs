//! CLI entry for `ai-buddy-verify`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use ai_buddy_verify::cleanup;
use ai_buddy_verify::contract::{Outcome, RunReport};
use ai_buddy_verify::doctor;
use ai_buddy_verify::overlay;
use ai_buddy_verify::paths::{self, RunPaths};
use ai_buddy_verify::poke;
use ai_buddy_verify::proof;
use ai_buddy_verify::summon;
use ai_buddy_verify::units;

#[derive(Debug, Parser)]
#[command(name = "ai-buddy-verify")]
#[command(about = "Agent/CI verify entry (doctor / units / overlay / poke / summon / cleanup)")]
#[command(version)]
struct Cli {
    /// Override evidence directory. Scratch is the sibling `scratch` under the
    /// same parent (pass `<run-root>/evidence` so layout matches the default).
    #[arg(long, global = true, value_name = "PATH")]
    evidence_dir: Option<PathBuf>,

    /// Run id used in the default root `$TMPDIR/ai-buddy-verify-$RUN_ID`.
    /// Default: UTC timestamp + process id.
    #[arg(long, global = true, value_name = "ID")]
    run_id: Option<String>,

    /// Print one machine-parseable result object on stdout instead of human progress.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Read-only readiness (layout, binary/cargo, OS tool SKIPs).
    Doctor,
    /// cargo test core + node tests + overlay-diagnostics script.
    Units,
    /// Dispatch platform verify-overlay leaf script; collect stamps into evidence.
    Overlay,
    /// Click the sprite for real; assert `verbs:.*Poke` in the app's trace.
    Poke,
    /// Double-click the sprite for real; assert `verbs:.*Summon`.
    Summon,
    /// Kill recorded PIDs; remove scratch; keep evidence.
    Cleanup,
}

impl Commands {
    fn name(&self) -> &'static str {
        match self {
            Commands::Doctor => "doctor",
            Commands::Units => "units",
            Commands::Overlay => "overlay",
            Commands::Poke => "poke",
            Commands::Summon => "summon",
            Commands::Cleanup => "cleanup",
        }
    }
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            // clap exits 2 on a usage error, and 2 is `skip` in this contract.
            // A bad flag proved nothing and broke nothing, it is a tool error.
            return if e.use_stderr() {
                ExitCode::from(Outcome::Error.exit_code())
            } else {
                ExitCode::SUCCESS
            };
        }
    };

    let paths = RunPaths::resolve(cli.evidence_dir, cli.run_id);
    let mut report = RunReport::new(cli.command.name(), &paths, cli.json);

    if let Err(e) = paths.ensure_dirs() {
        report.check(
            Outcome::Error,
            "evidence dirs",
            &format!("cannot create run dirs: {e}"),
        );
    } else {
        match paths::discover_repo_root() {
            Ok(repo_root) => match &cli.command {
                Commands::Doctor => doctor::run(&repo_root, &mut report),
                Commands::Units => units::run(&repo_root, &mut report),
                Commands::Overlay => overlay::run(&repo_root, &mut report),
                Commands::Poke => poke::run(&repo_root, &mut report),
                Commands::Summon => summon::run(&repo_root, &mut report),
                Commands::Cleanup => cleanup::run(&mut report),
            },
            // Doctor's product is the layout report, so it still runs and
            // records the layout failure itself.
            Err(e) => match &cli.command {
                Commands::Doctor => doctor::run(Path::new("."), &mut report),
                _ => report.check(Outcome::Error, "repo root", &e),
            },
        }
    }

    let _ = proof::append_proof(&report);
    report.emit();
    ExitCode::from(report.outcome().exit_code())
}
