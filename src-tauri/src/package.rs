//! Where Character Packages live, and how their bytes are got off a disk.
//!
//! `character::load` is a pure function from bytes to either a validated
//! Character or a list of an author's mistakes. Something has to open a
//! directory or an archive and hand it those bytes, and that something performs
//! I/O, so it belongs in the Shell rather than in `ai-buddy-core` — the same
//! split as `WindowSource`.
//!
//! The three ways a package can fail to become a Character are kept apart,
//! because they are three different problems for whoever has to fix them:
//!
//! - **Not a package** — this location holds no Character Manifest. A user
//!   pointed at their Downloads folder.
//! - **Unreadable** — the bytes could not be got at all: permissions, a
//!   truncated archive, a path that is not there.
//! - **Rejected** — the bytes were read and `character::load` refused them. The
//!   author has a package and a list of things to fix in it.
//!
//! A Character Package is untrusted input, so the reader is bounded before it
//! is convenient: a package cannot make ai-buddy read an unbounded number of
//! files, allocate an unbounded number of bytes, or walk an unbounded depth.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use ai_buddy_core::character::{self, Character, PackageBytes, CHARACTER_MANIFEST_FILE};

/// The file extension of a packaged Character. Zip because it is what a person
/// gets from Finder's "Compress", not because the format needs a container.
const ARCHIVE_EXTENSION: &str = "zip";

/// The most bytes a package may expand to, across every file in it.
///
/// A bound on us rather than on the author: an archive advertises its
/// uncompressed size for free, so without this a few kilobytes of zip can ask
/// for every byte of memory the machine has. Generous for a mascot — the whole
/// Required Animation Set at 1024x1024 is a fraction of it.
const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;

/// The most files a package may contain. Frames, a manifest, a prompt, and room
/// to spare for art nothing declares.
const MAX_PACKAGE_FILES: usize = 4096;

/// How many directories deep the reader will walk into a package, counting the
/// package root as the first. Anything nested deeper is refused rather than
/// walked.
const MAX_PACKAGE_DEPTH: usize = 8;

/// The environment variable that overrides where packages are looked for, as a
/// `:`-separated list of directories. Present for development and for the
/// verification script; a user never needs it.
pub const SEARCH_PATH_VAR: &str = "AI_BUDDY_CHARACTERS";

/// Why a location did not become a Character.
#[derive(Debug)]
pub enum ReadError {
    /// There is no Character Manifest here, so this was never a package.
    NotAPackage(PathBuf),
    /// The bytes could not be read at all.
    Unreadable { path: PathBuf, why: String },
    /// The bytes were read, and the loader refused them.
    Rejected { path: PathBuf, errors: Vec<String> },
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAPackage(path) => write!(
                f,
                "{} is not a Character Package: it contains no {CHARACTER_MANIFEST_FILE}",
                path.display()
            ),
            Self::Unreadable { path, why } => {
                write!(f, "{} could not be read: {why}", path.display())
            }
            Self::Rejected { path, errors } => {
                write!(f, "{} is not a valid Character Package:", path.display())?;
                for error in errors {
                    write!(f, "\n  - {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ReadError {}

/// A Character and the bytes it was loaded from.
///
/// The bytes are kept because a validated `Character` names its frames without
/// carrying them: the renderer and the hit-test both need the art itself, and
/// reading the package twice to get it would be the only alternative.
pub struct Package {
    pub character: Character,
    pub files: PackageBytes,
}

/// Read a Character Package from a directory or an archive.
pub fn read(path: &Path) -> Result<Package, ReadError> {
    let unreadable = |why: String| ReadError::Unreadable {
        path: path.to_path_buf(),
        why,
    };

    let metadata = fs::metadata(path).map_err(|e| unreadable(e.to_string()))?;
    let files = if metadata.is_dir() {
        read_directory(path)
    } else {
        read_archive(path)
    }
    .map_err(unreadable)?;

    let files = strip_single_root(files);
    if !files.contains_key(CHARACTER_MANIFEST_FILE) {
        return Err(ReadError::NotAPackage(path.to_path_buf()));
    }

    match character::load(&files) {
        Ok(character) => Ok(Package { character, files }),
        Err(errors) => Err(ReadError::Rejected {
            path: path.to_path_buf(),
            errors,
        }),
    }
}

/// Where ai-buddy looks for Character Packages, in the order it looks.
///
/// `bundled` is where the shipped Characters were installed alongside the app,
/// which only the Shell can say. A package the user added wins over a shipped
/// one of the same name, because the user's copy is the one they can edit.
pub fn search_paths(bundled: Option<PathBuf>) -> Vec<PathBuf> {
    if let Some(override_paths) = std::env::var_os(SEARCH_PATH_VAR) {
        return std::env::split_paths(&override_paths).collect();
    }

    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(
            PathBuf::from(home)
                .join("Library/Application Support/ai-buddy")
                .join("characters"),
        );
    }
    paths.extend(bundled);
    paths
}

/// Every Character Package visible in `search_paths`, in the order found.
///
/// A candidate rather than a Character: this only says "a directory or an
/// archive is here". Whether it is a package at all is `read`'s answer.
pub fn installed(search_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut packages = Vec::new();

    for directory in search_paths {
        let Ok(entries) = fs::read_dir(directory) else {
            continue; // a search path that does not exist is not an error
        };
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_dir()
                    || path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case(ARCHIVE_EXTENSION))
            })
            .collect();
        found.sort();
        packages.extend(found);
    }

    packages
}

/// Every file under `root`, keyed by its path relative to `root` with `/`
/// separators — the names a Character Manifest writes.
fn read_directory(root: &Path) -> Result<PackageBytes, String> {
    let mut files = PackageBytes::new();
    let mut budget = Budget::new();
    let mut pending = vec![(root.to_path_buf(), 0usize)];

    while let Some((directory, depth)) = pending.pop() {
        if depth >= MAX_PACKAGE_DEPTH {
            return Err(format!(
                "the package nests deeper than {MAX_PACKAGE_DEPTH} directories"
            ));
        }

        let entries = fs::read_dir(&directory)
            .map_err(|e| format!("{} could not be listed: {e}", directory.display()))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| format!("{} could not be read: {e}", directory.display()))?;
            let path = entry.path();

            // `symlink_metadata` rather than `metadata`: a link is described as
            // itself, so a package cannot reach outside its own directory by
            // pointing at somewhere else on the disk.
            let metadata = path
                .symlink_metadata()
                .map_err(|e| format!("{} could not be read: {e}", path.display()))?;

            if metadata.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            if !metadata.is_file() {
                continue; // a symlink, a socket, a device: not art and not a manifest
            }

            budget.charge(metadata.len())?;

            let Some(name) = relative_name(root, &path) else {
                continue;
            };
            let bytes = fs::read(&path)
                .map_err(|e| format!("{} could not be read: {e}", path.display()))?;
            files.insert(name, bytes);
        }
    }

    Ok(files)
}

/// Every file in a zip archive, keyed by the name it carries.
fn read_archive(path: &Path) -> Result<PackageBytes, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("not a readable archive: {e}"))?;

    let mut files = PackageBytes::new();
    let mut budget = Budget::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("entry {index} could not be read: {e}"))?;
        if !entry.is_file() {
            continue;
        }

        // `enclosed_name` refuses absolute paths and `..` components, so a
        // hostile archive cannot name a file outside the package. Nothing is
        // written to disk here, but the names it yields are the ones the
        // manifest resolves against, and those should mean what they say.
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(name) = name.to_str().map(|name| name.replace('\\', "/")) else {
            continue;
        };

        if is_macos_litter(&name) {
            continue;
        }

        // Charged against the size the archive advertises, before a byte is
        // decompressed, so a small archive claiming a huge entry is refused
        // rather than allocated for.
        let allowed = budget.charge(entry.size())?;

        // And read under that same cap, because the header is the archive's
        // own claim about itself. An entry declaring one byte and inflating to
        // a gigabyte is the ordinary shape of a zip bomb, and `read_to_end`
        // would follow it all the way down.
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(allowed + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{name} could not be read: {e}"))?;
        if bytes.len() as u64 > allowed {
            return Err(format!(
                "{name} decompresses to more than the {allowed} bytes it declares"
            ));
        }
        files.insert(name, bytes);
    }

    Ok(files)
}

/// Whether an archive entry is macOS bookkeeping rather than package content.
///
/// Finder's Compress writes a sibling `__MACOSX/` tree holding the extended
/// attributes of every file that carries any, and Finder drops a `.DS_Store` in
/// every folder it has displayed. Both describe the Mac the archive was made
/// on, not the Character, so they are dropped rather than offered to
/// `character::load` — and dropping the `__MACOSX/` tree is what leaves the
/// package under one top-level directory for `strip_single_root` to unwrap.
fn is_macos_litter(name: &str) -> bool {
    name.starts_with("__MACOSX/") || name == ".DS_Store" || name.ends_with("/.DS_Store")
}

/// What a package has left to spend. Each reader keeps its own.
///
/// One piece of arithmetic rather than one per reader: the two bounds have to
/// agree, and two copies of the same sum is how they stop agreeing.
struct Budget {
    bytes: u64,
    files: usize,
}

impl Budget {
    fn new() -> Self {
        Self {
            bytes: MAX_PACKAGE_BYTES,
            files: MAX_PACKAGE_FILES,
        }
    }

    /// Charge one file of `size` bytes, or refuse the package.
    ///
    /// Returns how many bytes the file may still occupy, so a reader that
    /// cannot trust the size it was told — an archive header, which the archive
    /// itself supplies — can cap what it decompresses at the same number.
    fn charge(&mut self, size: u64) -> Result<u64, String> {
        if self.files == 0 {
            return Err(format!(
                "the package holds more than {MAX_PACKAGE_FILES} files"
            ));
        }
        if size > self.bytes {
            return Err(format!(
                "the package expands to more than {} MiB",
                MAX_PACKAGE_BYTES / (1024 * 1024)
            ));
        }
        self.files -= 1;
        self.bytes -= size;
        Ok(size)
    }
}

/// The path of `file` relative to `root`, with `/` separators.
fn relative_name(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let parts: Vec<&str> = relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

/// Drop a single wrapping directory, if that is all the package has at its top.
///
/// `zip -r mochi.zip mochi/` — and Finder's Compress — put every file under one
/// directory named after the folder. Without this, the same package as a
/// directory and as an archive would not load identically, which is the whole
/// point of supporting both.
fn strip_single_root(files: PackageBytes) -> PackageBytes {
    if files.contains_key(CHARACTER_MANIFEST_FILE) {
        return files;
    }

    let mut roots = files.keys().filter_map(|name| name.split_once('/'));
    let Some((root, _)) = roots.next() else {
        return files;
    };
    let root = root.to_string();
    if files
        .keys()
        .any(|name| name.split_once('/').map(|(r, _)| r) != Some(root.as_str()))
    {
        return files;
    }

    files
        .into_iter()
        .filter_map(|(name, bytes)| {
            name.strip_prefix(&format!("{root}/"))
                .map(|stripped| (stripped.to_string(), bytes))
        })
        .collect::<BTreeMap<_, _>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A 2x2 RGBA PNG, which is all the loader asks of a frame.
    const FRAME: &[u8] = include_bytes!("../../crates/core/tests/fixtures/alpha-2x2.png");

    /// A directory of our own under the system temp dir, removed when the test
    /// ends. A handful of lines rather than a dev-dependency, as in `memory`.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir()
                .join(format!("ai-buddy-{label}-{}-{unique}", std::process::id()));
            fs::create_dir_all(&dir).expect("temp dir is creatable");
            Self(dir)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// The files of a minimal valid package: a manifest naming one frame per
    /// required Animation, the frames themselves under `frames/`, and a prompt.
    fn package_files() -> Vec<(String, Vec<u8>)> {
        let mut manifest = String::from("name = Blip\n");
        let mut files = Vec::new();

        for animation in character::REQUIRED_ANIMATIONS {
            manifest.push_str(&format!(
                "animation {animation} = frames/{animation}-0.png\n"
            ));
            files.push((format!("frames/{animation}-0.png"), FRAME.to_vec()));
        }

        files.push((CHARACTER_MANIFEST_FILE.to_string(), manifest.into_bytes()));
        files.push((
            character::PERSONALITY_FILE.to_string(),
            b"Blip is cheerful.".to_vec(),
        ));
        files
    }

    /// Write a package into `root` as a directory.
    fn write_package(root: &Path) {
        for (name, bytes) in package_files() {
            let path = root.join(&name);
            fs::create_dir_all(path.parent().expect("every file has a parent"))
                .expect("package directories are creatable");
            fs::write(&path, bytes).expect("package files are writable");
        }
    }

    /// Write the same package as an archive, wrapped in one directory the way
    /// `zip -r` and Finder's Compress both produce.
    ///
    /// With `macos_litter`, add what Finder writes on top of the package: the
    /// `__MACOSX/` shadow of every file, and a `.DS_Store`.
    fn write_archive(path: &Path, root_name: &str, macos_litter: bool) {
        let file = fs::File::create(path).expect("archive is creatable");
        let mut zip = zip::ZipWriter::new(file);
        let stored: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        let mut write = |name: String, bytes: &[u8]| {
            zip.start_file(name, stored)
                .expect("archive entry is startable");
            zip.write_all(bytes).expect("archive entry is writable");
        };

        for (name, bytes) in package_files() {
            write(format!("{root_name}/{name}"), &bytes);
            if macos_litter {
                let (directory, file) = match name.rsplit_once('/') {
                    Some((directory, file)) => (format!("{directory}/"), file),
                    None => (String::new(), name.as_str()),
                };
                write(
                    format!("__MACOSX/{root_name}/{directory}._{file}"),
                    b"extended attributes, not art",
                );
            }
        }
        if macos_litter {
            write(format!("{root_name}/.DS_Store"), b"folder bookkeeping");
        }
        zip.finish().expect("archive is finishable");
    }

    /// The rejection, or a failure naming the Character that loaded instead.
    ///
    /// `Package` carries every byte of the art, so it is never `Debug`-printed.
    fn refusal(result: Result<Package, ReadError>) -> ReadError {
        match result {
            Ok(package) => panic!("expected a refusal, loaded {}", package.character.name),
            Err(why) => why,
        }
    }

    #[test]
    fn a_package_directory_loads_into_a_validated_character() {
        let dir = TempDir::new("package-dir");
        let root = dir.join("blip");
        write_package(&root);

        let character = read(&root).expect("the package is valid").character;
        assert_eq!(character.name, "Blip");
        assert_eq!(character.personality, "Blip is cheerful.");
        assert_eq!(
            character.animations.len(),
            character::REQUIRED_ANIMATIONS.len()
        );
        assert!(
            character.animations["walk"].frames == vec!["frames/walk-0.png".to_string()],
            "frames keep the names the manifest wrote"
        );
    }

    #[test]
    fn the_same_package_as_an_archive_loads_identically() {
        let dir = TempDir::new("package-archive");
        let root = dir.join("blip");
        write_package(&root);
        let archive = dir.join("blip.zip");
        write_archive(&archive, "blip", false);

        let from_directory = read(&root).expect("the directory is valid");
        let from_archive = read(&archive).expect("the archive is valid");
        assert_eq!(from_directory.character, from_archive.character);
        assert_eq!(from_directory.files, from_archive.files);
    }

    /// What an author actually hands you, since Finder's Compress is why this
    /// reads zip at all. The `__MACOSX/` tree is a second top-level directory,
    /// so leaving it in place hides the Character Manifest one level down and
    /// the package is disowned.
    #[test]
    fn an_archive_carrying_finders_litter_loads_identically() {
        let dir = TempDir::new("finder-archive");
        let root = dir.join("blip");
        write_package(&root);
        let archive = dir.join("blip.zip");
        write_archive(&archive, "blip", true);

        let from_directory = read(&root).expect("the directory is valid");
        let from_archive = read(&archive).expect("the archive is valid");
        assert_eq!(from_directory.character, from_archive.character);
        assert_eq!(
            from_directory.files, from_archive.files,
            "the litter is dropped rather than carried as package content"
        );
    }

    /// The depth bound is the one the constant names: the package root and
    /// seven directories under it are walked, and an eighth is refused.
    #[test]
    fn a_package_nested_past_the_depth_bound_is_refused() {
        let dir = TempDir::new("deep");
        let root = dir.join("blip");
        let deepest = root.join("1/2/3/4/5/6/7");
        fs::create_dir_all(&deepest).expect("directories are creatable");
        read_directory(&root).expect("the bound itself is walked");

        fs::create_dir_all(deepest.join("8")).expect("directories are creatable");
        let why = read_directory(&root).expect_err("one directory deeper is refused");
        assert!(why.contains("nests deeper"), "{why}");
    }

    #[test]
    fn a_directory_that_is_not_a_package_is_reported_as_such() {
        let dir = TempDir::new("not-a-package");
        let downloads = dir.join("Downloads");
        fs::create_dir_all(&downloads).expect("directory is creatable");
        fs::write(downloads.join("invoice.pdf"), b"not art").expect("file is writable");

        match refusal(read(&downloads)) {
            ReadError::NotAPackage(path) => assert_eq!(path, downloads),
            other => panic!("expected NotAPackage, got {other:?}"),
        }
    }

    /// The distinction the author cares about: a package with a mistake in it
    /// is a different problem from a directory that was never a package.
    #[test]
    fn a_package_missing_a_required_animation_is_rejected_rather_than_disowned() {
        let dir = TempDir::new("broken-package");
        let root = dir.join("blip");
        write_package(&root);
        fs::write(
            root.join(CHARACTER_MANIFEST_FILE),
            "name = Blip\nanimation idle = frames/idle-0.png\n",
        )
        .expect("manifest is writable");

        match refusal(read(&root)) {
            ReadError::Rejected { path, errors } => {
                assert_eq!(path, root);
                assert!(
                    errors.iter().any(|error| error.contains("\"walk\"")),
                    "the rejection names the missing animation: {errors:#?}"
                );
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn a_truncated_archive_is_reported_with_its_path() {
        let dir = TempDir::new("truncated");
        let archive = dir.join("blip.zip");
        fs::write(&archive, b"PK\x03\x04 and then nothing").expect("file is writable");

        match refusal(read(&archive)) {
            ReadError::Unreadable { path, why } => {
                assert_eq!(path, archive);
                assert!(!why.is_empty(), "the reason is carried, not dropped");
            }
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    #[test]
    fn a_path_that_is_not_there_is_reported_with_its_path() {
        let dir = TempDir::new("absent");
        let missing = dir.join("nowhere");

        match refusal(read(&missing)) {
            ReadError::Unreadable { path, .. } => assert_eq!(path, missing),
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    /// A zip bomb: an entry whose header declares almost nothing and whose
    /// stream inflates to far more. The header is the archive's claim about
    /// itself, so the reader has to cap the read rather than believe it.
    #[test]
    fn an_archive_entry_that_inflates_past_what_it_declares_is_refused() {
        let dir = TempDir::new("zip-bomb");
        let archive = dir.join("bomb.zip");

        {
            let file = fs::File::create(&archive).expect("archive is creatable");
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file(
                CHARACTER_MANIFEST_FILE,
                zip::write::FileOptions::<'_, ()>::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .expect("archive entry is startable");
            // Highly compressible, so the entry on disk is a few hundred bytes.
            zip.write_all(&vec![b'a'; 8 * 1024 * 1024])
                .expect("archive entry is writable");
            zip.finish().expect("archive is finishable");
        }

        // Rewrite the declared uncompressed size to 1 everywhere it appears:
        // the local header, the central directory, and any data descriptor.
        let mut bytes = fs::read(&archive).expect("archive is readable");
        let declared = (8u32 * 1024 * 1024).to_le_bytes();
        for at in 0..bytes.len().saturating_sub(4) {
            if bytes[at..at + 4] == declared {
                bytes[at..at + 4].copy_from_slice(&1u32.to_le_bytes());
            }
        }
        fs::write(&archive, bytes).expect("archive is writable");

        match refusal(read(&archive)) {
            ReadError::Unreadable { path, why } => {
                assert_eq!(path, archive);
                assert!(
                    why.contains("declares"),
                    "the refusal says the entry outgrew its header: {why}"
                );
            }
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    #[test]
    fn a_file_that_cannot_be_opened_is_reported_with_its_path() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("permissions");
        let root = dir.join("blip");
        write_package(&root);

        let unreadable = root.join("frames/idle-0.png");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))
            .expect("permissions are settable");

        let refused = refusal(read(&root));
        // Put it back before asserting, so a failure still leaves a removable
        // directory behind.
        let _ = fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644));

        match refused {
            ReadError::Unreadable { path, why } => {
                assert_eq!(path, root, "the package is named");
                assert!(
                    why.contains("idle-0.png"),
                    "and so is the file inside it that could not be opened: {why}"
                );
            }
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    #[test]
    fn installed_packages_are_the_directories_and_archives_in_the_search_paths() {
        let dir = TempDir::new("installed");
        let characters = dir.join("characters");
        fs::create_dir_all(characters.join("blip")).expect("directory is creatable");
        fs::write(characters.join("mochi.zip"), b"").expect("file is writable");
        fs::write(characters.join("README.md"), b"").expect("file is writable");

        let found = installed(&[characters.clone(), dir.join("no-such-directory")]);
        assert_eq!(
            found,
            vec![characters.join("blip"), characters.join("mochi.zip")],
            "a directory and an archive count; a loose file does not, \
             and a search path that is not there is not an error"
        );
    }
}
