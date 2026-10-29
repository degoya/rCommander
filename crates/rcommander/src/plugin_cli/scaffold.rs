//! `rcommander plugin new`: a plugin from the SDK template of its kind, with its own id and key
//! pair (RC-100-12).
//!
//! Copied and adapted from rDownloader's `crates/rd-pack/src/plugin_scaffold.rs` at commit
//! 26c8592a6: the scaffold is a real, compiling plugin rather than a sketch with holes in it, so
//! an author builds, packages and passes conformance before changing a line, and a later failure
//! is about their code. Adapted: the templates are compiled into the executable rather than read
//! from a checkout, because rCommander ships as a zip without `sdk/`; `scaffold_tests.rs` holds
//! the embedded files to the folder.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};

/// The kinds `plugin new` can scaffold: one per world of the contract that has a template.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum PluginKind {
    /// Shows a file in the Lister and Quick View.
    Viewer,
    /// A password manager as the source of a connection's secret (contract 0.3.0).
    Secrets,
}

impl PluginKind {
    /// The template's files, as `(path, text)`.
    fn files(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Viewer => VIEWER,
            Self::Secrets => SECRETS,
        }
    }
}

/// `sdk/templates/viewer`, file by file.
pub(super) const VIEWER: &[(&str, &str)] = &[
    (
        "CHANGES.md",
        include_str!("../../../../sdk/templates/viewer/CHANGES.md"),
    ),
    (
        "Cargo.toml",
        include_str!("../../../../sdk/templates/viewer/Cargo.toml"),
    ),
    (
        "README.md",
        include_str!("../../../../sdk/templates/viewer/README.md"),
    ),
    (
        "locales/en.json",
        include_str!("../../../../sdk/templates/viewer/locales/en.json"),
    ),
    (
        "manifest.toml",
        include_str!("../../../../sdk/templates/viewer/manifest.toml"),
    ),
    (
        "src/guest.rs",
        include_str!("../../../../sdk/templates/viewer/src/guest.rs"),
    ),
    (
        "src/lib.rs",
        include_str!("../../../../sdk/templates/viewer/src/lib.rs"),
    ),
    (
        "src/lib_tests.rs",
        include_str!("../../../../sdk/templates/viewer/src/lib_tests.rs"),
    ),
    (
        "src/table.rs",
        include_str!("../../../../sdk/templates/viewer/src/table.rs"),
    ),
    (
        "src/types.rs",
        include_str!("../../../../sdk/templates/viewer/src/types.rs"),
    ),
    (
        "wit/rcommander.wit",
        include_str!("../../../../sdk/templates/viewer/wit/rcommander.wit"),
    ),
];

/// `sdk/templates/secrets`, file by file.
pub(super) const SECRETS: &[(&str, &str)] = &[
    (
        "CHANGES.md",
        include_str!("../../../../sdk/templates/secrets/CHANGES.md"),
    ),
    (
        "Cargo.toml",
        include_str!("../../../../sdk/templates/secrets/Cargo.toml"),
    ),
    (
        "README.md",
        include_str!("../../../../sdk/templates/secrets/README.md"),
    ),
    (
        "locales/en.json",
        include_str!("../../../../sdk/templates/secrets/locales/en.json"),
    ),
    (
        "manifest.toml",
        include_str!("../../../../sdk/templates/secrets/manifest.toml"),
    ),
    (
        "src/guest.rs",
        include_str!("../../../../sdk/templates/secrets/src/guest.rs"),
    ),
    (
        "src/lib.rs",
        include_str!("../../../../sdk/templates/secrets/src/lib.rs"),
    ),
    (
        "src/lib_tests.rs",
        include_str!("../../../../sdk/templates/secrets/src/lib_tests.rs"),
    ),
    (
        "src/types.rs",
        include_str!("../../../../sdk/templates/secrets/src/types.rs"),
    ),
    (
        "wit/rcommander.wit",
        include_str!("../../../../sdk/templates/secrets/wit/rcommander.wit"),
    ),
];

/// The scaffold's private signing key, next to its manifest.
pub(super) const SIGNING_KEY_FILE: &str = "plugin-signing.key";

/// What a scaffold keeps out of version control: the private key, the build output and the
/// packages built from it.
const IGNORED: [&str; 3] = [SIGNING_KEY_FILE, "target/", "*.rcplug"];

/// The longest plugin name `plugin new` takes.
const MAX_NAME_CHARS: usize = 64;

/// What `plugin new` wrote.
#[derive(Debug)]
pub struct Scaffolded {
    pub name: String,
    pub slug: String,
    pub id: String,
    pub key_path: PathBuf,
}

/// Writes a plugin of `kind` into `out`, which must not exist, named `name` or after `out`.
///
/// # Errors
///
/// `out` exists, the name makes no usable crate name, or a file cannot be written.
pub fn scaffold(kind: PluginKind, out: &Path, name: Option<&str>) -> Result<Scaffolded> {
    ensure!(
        !out.exists(),
        "{} already exists; choose a folder that does not",
        out.display()
    );
    let name = match name {
        Some(name) => name.to_owned(),
        None => out
            .file_name()
            .and_then(|value| value.to_str())
            .context("--out needs a folder name")?
            .to_owned(),
    };
    check_name(&name)?;
    let slug = slug(&name)?;
    // Every scaffold gets its own identity: two plugins sharing an id would fight over the same
    // installation folder.
    let id = uuid::Uuid::now_v7().to_string();
    // And its own key pair, so it can be built, packaged and verified before a line of it is
    // changed.
    let key = rc_plugin_host::generate_signing_key()?;
    for (path, text) in kind.files() {
        let filled = text
            .replace("{{PLUGIN_NAME}}", &name)
            .replace("{{PLUGIN_SLUG}}", &slug)
            .replace("{{PLUGIN_ID}}", &id)
            .replace(
                "REPLACE_WITH_YOUR_BASE64_ED25519_PUBLIC_KEY",
                &key.public_base64,
            );
        let target = out.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        std::fs::write(&target, filled).with_context(|| format!("write {}", target.display()))?;
    }
    // The ignore rules go in before the key does: `git add .` in a fresh plugin repository must
    // not commit the private key (rDownloader's audit K1).
    write_gitignore(out)?;
    let key_path = out.join(SIGNING_KEY_FILE);
    write_private(&key_path, key.private_pem.as_bytes())?;
    Ok(Scaffolded {
        name,
        slug,
        id,
        key_path,
    })
}

/// A name that goes into TOML and JSON strings as it is: no quote, backslash or control
/// character, at most [`MAX_NAME_CHARS`].
fn check_name(name: &str) -> Result<()> {
    let length = name.chars().count();
    ensure!(
        (1..=MAX_NAME_CHARS).contains(&length),
        "the plugin name needs 1 to {MAX_NAME_CHARS} characters"
    );
    if let Some(bad) = name
        .chars()
        .find(|character| matches!(character, '"' | '\\') || character.is_control())
    {
        bail!("the plugin name may not hold {bad:?}");
    }
    Ok(())
}

/// The crate name and key name of `name`: ASCII letters and digits lower-cased, anything else an
/// underscore, starting with a letter.
pub(super) fn slug(name: &str) -> Result<String> {
    let slug: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    ensure!(
        slug.len() >= 2 && slug.starts_with(|character: char| character.is_ascii_lowercase()),
        "`{name}` makes no usable crate name; pass --name with a name that starts with a letter"
    );
    Ok(slug)
}

/// Adds [`IGNORED`] to the scaffold's `.gitignore`, keeping whatever the template put there.
pub(super) fn write_gitignore(directory: &Path) -> Result<()> {
    let path = directory.join(".gitignore");
    let mut text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    for entry in IGNORED {
        if text.lines().any(|line| line.trim() == entry) {
            continue;
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(entry);
        text.push('\n');
    }
    std::fs::write(&path, text).with_context(|| format!("write {}", path.display()))
}

/// Writes a private key to a new file, readable by its owner only where the file system says so;
/// an existing file is never replaced.
pub(super) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create {} (it must not exist)", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
#[path = "scaffold_tests.rs"]
mod tests;
