//! The Windows credential manager as a built-in secret source, `wincred` (RC-100-37).
//!
//! A WebAssembly plugin reaches no Windows API (`docs/plugins.md`), so this source is part of the
//! core, behind the same seam as the plugin sources. It reads the person's *generic* credentials
//! only and never writes: a search sees the target name, the user and the comment, never the
//! blob; `get` reads the blob of one target. Creating and changing entries stays with Windows.
//! Other platforms have no such source: it is not offered, and a reference to it answers
//! `secret_source_missing`.
//!
//! The blob holds whatever the program that stored it chose: Windows' own dialog, `cmdkey` and
//! most libraries write UTF-16LE, Git Credential Manager writes UTF-8. [`decode`] tells them
//! apart by the NUL bytes UTF-16 of any Latin text has and UTF-8 never has.

use rc_plugin_api::secrets::SecretEntry;
use rc_plugin_host::{
    MAX_ENTRY_ID_CHARS, MAX_ENTRY_TEXT_CHARS, MAX_ENTRY_TITLE_CHARS, MAX_QUERY_CHARS, codes,
    readable,
};
use rc_plugin_types::{Failure, FailureKind};
use rc_vault::SecretString;
use zeroize::Zeroizing;

#[cfg(windows)]
mod windows;

/// The name the person knows the source by.
pub(crate) const NAME: &str = "Windows Credential Manager";

/// The setting that offers the source on Windows (`vault.wincred_source`).
pub const WINCRED_SOURCE_KEY: &str = "vault.wincred_source";

/// Whether this platform has the source at all.
pub(crate) const PLATFORM: bool = cfg!(windows);

/// One generic credential, without its blob.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "only Windows lists credentials")
)]
pub(crate) struct Credential {
    pub(crate) target: String,
    pub(crate) user: Option<String>,
    pub(crate) comment: Option<String>,
}

/// At most `limit` generic credentials whose target, user or comment contains `query`, ignoring
/// case; every one for an empty query. A target no reference could name is left out. A query
/// is bounded as a plugin's is.
pub(crate) fn search(query: &str, limit: u32) -> Result<Vec<SecretEntry>, Failure> {
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(Failure::coded(
            FailureKind::BadData,
            codes::PLUGIN_REQUEST_TOO_LARGE,
        ));
    }
    let wanted = query.to_lowercase();
    let mut entries: Vec<SecretEntry> = credentials()?
        .into_iter()
        .filter(|credential| usable_target(&credential.target))
        .filter(|credential| {
            [
                Some(&credential.target),
                credential.user.as_ref(),
                credential.comment.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.to_lowercase().contains(&wanted))
        })
        .map(|credential| SecretEntry {
            title: readable(&credential.target, MAX_ENTRY_TITLE_CHARS),
            user: credential
                .user
                .map(|user| readable(&user, MAX_ENTRY_TEXT_CHARS)),
            url: None,
            id: credential.target,
        })
        .collect();
    entries.sort_by_key(|entry| entry.id.to_lowercase());
    entries.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    Ok(entries)
}

/// The secret stored under the generic credential `target`.
pub(crate) fn get(target: &str) -> Result<SecretString, Failure> {
    if !usable_target(target) {
        return Err(Failure::new(FailureKind::BadData));
    }
    let blob = read(target)?;
    decode(&blob)
        .filter(|value| !value.is_empty())
        .map(SecretString::from)
        .ok_or_else(|| Failure::new(FailureKind::NotFound))
}

/// Whether a reference can name `target`: within the entry id's bounds, no control character.
fn usable_target(target: &str) -> bool {
    !target.is_empty()
        && target.chars().count() <= MAX_ENTRY_ID_CHARS
        && !target.chars().any(char::is_control)
}

/// The blob as text: UTF-8 when it is valid UTF-8 without a NUL byte, otherwise UTF-16LE with a
/// closing NUL dropped. `None` when it is neither.
pub(crate) fn decode(blob: &[u8]) -> Option<String> {
    if !blob.contains(&0)
        && let Ok(text) = std::str::from_utf8(blob)
    {
        return Some(text.to_owned());
    }
    if !blob.len().is_multiple_of(2) {
        return None;
    }
    let mut units: Zeroizing<Vec<u16>> = Zeroizing::new(
        blob.as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect(),
    );
    while units.last() == Some(&0) {
        units.pop();
    }
    String::from_utf16(&units).ok()
}

#[cfg(windows)]
fn credentials() -> Result<Vec<Credential>, Failure> {
    windows::credentials()
}

#[cfg(windows)]
fn read(target: &str) -> Result<Zeroizing<Vec<u8>>, Failure> {
    windows::read(target)
}

#[cfg(not(windows))]
fn credentials() -> Result<Vec<Credential>, Failure> {
    Err(Failure::new(FailureKind::NotSupported))
}

#[cfg(not(windows))]
fn read(_target: &str) -> Result<Zeroizing<Vec<u8>>, Failure> {
    Err(Failure::new(FailureKind::NotSupported))
}

#[cfg(test)]
#[path = "wincred_tests.rs"]
mod tests;
