//! What a secret source asks and answers, held to the host's rules (`docs/plugins.md`,
//! "Validation of plugin output"): ids it can be asked for again exactly, labels made safe to
//! show, a value of a bounded size.

use rc_plugin_api::secrets::{SecretEntry, SecretField, SourceState};
use rc_plugin_types::{Failure, FailureKind};
use secrecy::SecretString;
use zeroize::Zeroizing;

use crate::{
    bindings::secrets_world::exports::rcommander::plugin::secrets as wit,
    failure::{self, codes, readable},
};

/// The most entries one search answers.
pub const MAX_SEARCH_ENTRIES: u32 = 100;
/// The longest query handed to a plugin.
pub const MAX_QUERY_CHARS: usize = 256;
/// The longest entry id: handed back to `get` exactly, so it is never cut.
pub const MAX_ENTRY_ID_CHARS: usize = 256;
/// The longest title, user and address shown; longer ones are cut.
pub const MAX_ENTRY_TITLE_CHARS: usize = 200;
pub const MAX_ENTRY_TEXT_CHARS: usize = 512;
/// The largest secret `get` answers: a private key in PEM fits many times over (64 KiB).
pub const MAX_SECRET_BYTES: usize = 64 * 1024;

/// Whether `id` can be handed to a plugin and back: not empty, not too long, no control
/// character.
pub(super) fn entry_id(id: &str) -> bool {
    !id.is_empty() && id.chars().count() <= MAX_ENTRY_ID_CHARS && !id.chars().any(char::is_control)
}

pub(super) fn state(state: wit::SourceState) -> SourceState {
    match state {
        wit::SourceState::Ready => SourceState::Ready,
        wit::SourceState::Locked => SourceState::Locked,
        wit::SourceState::Unreachable => SourceState::Unreachable,
        wit::SourceState::Unlicensed => SourceState::Unlicensed,
    }
}

pub(super) fn field(field: SecretField) -> wit::SecretField {
    match field {
        SecretField::Password => wit::SecretField::Password,
        SecretField::PrivateKey => wit::SecretField::PrivateKey,
        SecretField::Passphrase => wit::SecretField::Passphrase,
        SecretField::Totp => wit::SecretField::Totp,
    }
}

/// A search's answer: at most `limit` entries, each with an id that goes back exactly; titles,
/// users and addresses cut and their control characters spelled out.
pub(super) fn entries(raw: Vec<wit::SecretEntry>, limit: u32) -> Result<Vec<SecretEntry>, Failure> {
    let bad = || failure::coded(FailureKind::BadData, codes::PLUGIN_BAD_OUTPUT);
    if raw.len() > limit as usize {
        return Err(bad());
    }
    let shown = |text: Option<String>| {
        text.filter(|text| !text.trim().is_empty())
            .map(|text| readable(&text, MAX_ENTRY_TEXT_CHARS))
    };
    raw.into_iter()
        .map(|entry| {
            if !entry_id(&entry.id) {
                return Err(bad());
            }
            Ok(SecretEntry {
                id: entry.id,
                title: readable(&entry.title, MAX_ENTRY_TITLE_CHARS),
                user: shown(entry.user),
                url: shown(entry.url),
            })
        })
        .collect()
}

/// The value `get` answered, as a secret: never empty, never larger than
/// [`MAX_SECRET_BYTES`], no NUL. The string handed in is wiped, also when it is refused; the
/// secret is an exact copy, so making it moves no bytes into a buffer nobody wipes.
pub(super) fn secret(value: String) -> Result<SecretString, Failure> {
    let value = Zeroizing::new(value);
    if value.len() > MAX_SECRET_BYTES {
        return Err(failure::coded(
            FailureKind::BadData,
            codes::PLUGIN_OUTPUT_TOO_LARGE,
        ));
    }
    if value.is_empty() || value.contains('\0') {
        return Err(failure::coded(
            FailureKind::BadData,
            codes::PLUGIN_BAD_OUTPUT,
        ));
    }
    Ok(SecretString::from(value.as_str()))
}
