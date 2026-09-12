//! CLI entry for `ai-buddy-verify`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use ai_buddy_verify::cleanup;
use ai_buddy_verify::doctor;
use ai_buddy_verify::overlay;
use ai_buddy_verify::paths::{self, RunPaths};
use ai_buddy_verify::units;

#[derive(Debug, Parser)]
#[command(name = "ai-buddy-verify")]
#[command(about = "Agent/CI verify entry (doctor / units / overlay / cleanup)")]
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

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Read-only readiness (layout, binary/cargo, OS tool WARNs).
    Doctor,
    /// cargo test core + node tests + overlay-diagnostics script.
    Units,
    /// Dispatch platform verify-overlay leaf script; collect stamps into evidence.
    Overlay,
    /// Kill recorded PIDs; remove scratch; keep evidence.
    Cleanup,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let paths = RunPaths::resolve(cli.evidence_dir, cli.run_id);
    if let Err(e) = paths.ensure_dirs() {
        eprintln!("ai-buddy-verify: cannot create run dirs: {e}");
        return ExitCode::from(1);
    }

    let repo_root = match paths::discover_repo_root() {
        Ok(r) => r,
        Err(e) => {
            // doctor still wants to report layout fail; others need the root.
            eprintln!("ai-buddy-verify: {e}");
            match cli.command {
                Commands::Doctor => {
                    let code = doctor::run(std::path::Path::new("."), &paths);
                    return ExitCode::from(code as u8);
                }
                _ => return ExitCode::from(1),
            }
        }
    };

    let code = match cli.command {
        Commands::Doctor => doctor::run(&repo_root, &paths),
        Commands::Units => units::run(&repo_root, &paths),
        Commands::Overlay => overlay::run(&repo_root, &paths),
        Commands::Cleanup => cleanup::run(&paths),
    };
    ExitCode::from(code as u8)
}
