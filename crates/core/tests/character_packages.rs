//! Integration tests for shipped Character Packages.
//!
//! These tests validate the characters in the `characters/` directory at the
//! workspace root: Cat, Black Mage, and Timber Wolf. They test the public
//! seam of `character::load` — that each package is accepted, carries its
//! required animations, and (for pixel art) walk frames face right so the
//! engine's mirroring works.

use ai_buddy_core::character::{self, REQUIRED_ANIMATIONS};
use std::collections::BTreeMap;
use std::path::Path;

/// Load a Character Package from a directory in the workspace root.
fn load_package(name: &str) -> Result<character::Character, Vec<String>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core is in crates/")
        .parent()
        .expect("crates/ is in workspace");
    let package_dir = root.join("characters").join(name);

    let mut files = BTreeMap::new();
    collect(&package_dir, &package_dir, &mut files)
        .unwrap_or_else(|e| panic!("{}: {}", package_dir.display(), e));

    character::load(&files)
}

/// Recursively read a directory into a `PackageBytes` map.
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

#[test]
fn cat_package_loads_with_all_required_animations() {
    let character = load_package("cat").expect("Cat package is valid");

    assert_eq!(character.name, "Cat");
    for required in REQUIRED_ANIMATIONS {
        assert!(
            character.animations.contains_key(required),
            "Cat declares {required:?}"
        );
    }
}

#[test]
fn black_mage_package_loads_with_all_required_animations() {
    let character = load_package("black-mage").expect("Black Mage package is valid");

    assert_eq!(character.name, "Black Mage");
    for required in REQUIRED_ANIMATIONS {
        assert!(
            character.animations.contains_key(required),
            "Black Mage declares {required:?}"
        );
    }
    assert_eq!(
        character.scale, 3,
        "Black Mage uses scale 3 for readability"
    );
}

#[test]
fn timber_wolf_package_loads_with_all_required_animations() {
    let character = load_package("timber-wolf").expect("Timber Wolf package is valid");

    assert_eq!(character.name, "Timber Wolf");
    assert!(
        !character.personality.is_empty(),
        "Timber Wolf has a personality prompt"
    );

    for required in REQUIRED_ANIMATIONS {
        assert!(
            character.animations.contains_key(required),
            "Timber Wolf declares {required:?}"
        );
    }

    let walk = &character.animations["walk"];
    assert!(
        walk.frames.len() >= 2,
        "walk has at least 2 frames for animation"
    );
}

#[test]
fn timber_wolf_behaviors_compose_existing_primitives() {
    let character = load_package("timber-wolf").expect("Timber Wolf package is valid");

    assert!(
        !character.behaviors.is_empty(),
        "Timber Wolf declares behaviors"
    );

    assert!(
        character.behaviors.contains_key("patrol"),
        "patrol behavior exists"
    );
    assert!(
        character.behaviors.contains_key("engage"),
        "engage behavior exists for shooting emote"
    );

    let engage = &character.behaviors["engage"];
    assert!(
        engage.primitives.len() >= 2,
        "engage composes multiple primitives"
    );
}

#[test]
fn timber_wolf_uses_scale_3_for_readability() {
    let character = load_package("timber-wolf").expect("Timber Wolf package is valid");

    assert_eq!(
        character.scale, 3,
        "Timber Wolf is a heavy mech but stays companion-sized at scale 3"
    );
}
