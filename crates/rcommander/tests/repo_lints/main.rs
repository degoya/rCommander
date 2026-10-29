//! The repository's own rules as tests, one binary for all of them so a run links it once:
//!
//! * `file_length` - at most 500 lines per Rust source and per script.
//! * `no_german` - Rust sources are English-only.
//! * `no_console_window` - every spawn asks for no console window on Windows.
//! * `i18n_keys` - every `t!` key is in `locales/en.json`, every key there is used, none twice.
//! * `focus_ring` - every checkbox, toggler, pick list, radio group, slider and hand-drawn
//!   button of a dialog is a stop of its focus ring.
//!
//! `scan` is what they read of a source: the code without comments and texts, calls and the
//! statements they stand in.

use std::path::{Path, PathBuf};

mod file_length;
mod focus_ring;
mod i18n_keys;
mod no_console_window;
mod no_german;
mod scan;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Every file below `directory` whose name ends in one of `extensions`, skipping the workspace's
/// build directory (`<root>/target`) and hidden directories. A folder named `target` anywhere
/// else is source like any other and is read (RC-080-54).
fn files_with(directory: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    walk(directory, extensions, &workspace_root().join("target"), out);
}

fn walk(directory: &Path, extensions: &[&str], build: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_dir() {
            if !name.starts_with('.') && path != build {
                walk(&path, extensions, build, out);
            }
        } else if kind.is_file() && extensions.iter().any(|ext| name.ends_with(ext)) {
            out.push(path);
        }
    }
}

/// The old walk skipped every folder named `target`, so `crates/x/src/target/` went unread
/// (RC-080-54); only the build directory it is given is skipped now.
#[test]
fn only_the_build_directory_is_skipped() {
    let root = tempfile::tempdir().expect("tempdir");
    let build = root.path().join("target");
    for file in [
        "target/debug/built.rs",
        "src/target/kept.rs",
        "src/lib.rs",
        ".hidden/x.rs",
    ] {
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(&path, "").expect("writes");
    }
    let mut found = Vec::new();
    walk(root.path(), &[".rs"], &build, &mut found);
    let mut found: Vec<String> = found
        .iter()
        .map(|path| relative(root.path(), path))
        .collect();
    found.sort();
    assert_eq!(found, ["src/lib.rs", "src/target/kept.rs"]);
}

/// `path` relative to the workspace root, with `/` on every platform.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
