//! Validate a Character Package directory with `character::load`.
//!
//! ```sh
//! cargo run -p ai-buddy-core --example validate -- characters/cat
//! ```
//!
//! `scripts/import-pet.py` runs this before declaring an import a success;
//! the shell's own reader adds archive and size handling this example does
//! not need, so the walk here stays a plain recursive read.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use ai_buddy_core::character;

fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else {
            let name = path
                .strip_prefix(root)
                .expect("every walked path starts at the root")
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.insert(name, std::fs::read(&path)?);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: validate <package-directory>");
        return ExitCode::FAILURE;
    };
    let root = Path::new(&root);
    let mut files = BTreeMap::new();
    if let Err(why) = collect(root, root, &mut files) {
        eprintln!("{}: {why}", root.display());
        return ExitCode::FAILURE;
    }
    match character::load(&files) {
        Ok(character) => {
            println!("{} is a valid Character Package", character.name);
            ExitCode::SUCCESS
        }
        Err(errors) => {
            eprintln!("{} is not a valid Character Package:", root.display());
            for error in errors {
                eprintln!("  - {error}");
            }
            ExitCode::FAILURE
        }
    }
}
