//! What the parser never reads: key material. A file whose name is a key's or whose text holds a
//! key stops the whole import, the configuration itself as much as a file it includes, so that
//! no part of a key can reach a warning, a host or an answer.

use std::{io, path::Path, path::PathBuf};

/// Why a configuration was not read.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The configuration itself cannot be read.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// The configuration, or a file it includes, is or holds key material.
    #[error("{} is key material, not an SSH configuration; nothing was imported", file.display())]
    KeyMaterial { file: PathBuf },
}

/// File names of keys and of the files beside them that are no configuration.
const KEY_NAMES: &[&str] = &["authorized_keys", "authorized_keys2", "known_hosts"];

/// Extensions of key files.
const KEY_EXTENSIONS: &[&str] = &["pem", "key", "ppk", "pub"];

/// Whether `path` names a key file: `id_*`, `*.pem`, `*.key`, `*.ppk`, `*.pub`,
/// `authorized_keys`, `authorized_keys2` or `known_hosts` (ignoring ASCII case).
#[must_use]
pub fn is_key_file(path: &Path) -> bool {
    let Some(name) = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
    else {
        return false;
    };
    name.starts_with("id_")
        || KEY_NAMES.contains(&name.as_str())
        || name
            .rsplit_once('.')
            .is_some_and(|(_, extension)| KEY_EXTENSIONS.contains(&extension))
}

/// Whether `text` holds key material: PEM armour (OpenSSH, PKCS#1/#8, a certificate) or a
/// private key marker of the one table (`rc_remote::key_markers`, RC-080-43).
#[must_use]
pub fn holds_key(text: &str) -> bool {
    rc_remote::key_markers::holds_key_material(text)
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
