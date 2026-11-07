//! `{{secret:<slot>}}` in an `http` request (contract 0.4.0, RC-100-39): an inventory names the
//! API token the person stored for its source without ever holding it, and the host fills it in
//! on the way out (`docs/plugins.md`, "Sending a credential you never see").
//!
//! Adapted from rDownloader's `crates/rd-plugin-host/src/native/expand.rs` and `references.rs`
//! (commit 26c8592a6), narrowed to what an inventory needs: markers in header values only. A
//! marker in the address or the body, or in a header that travels to third parties (`Referer`,
//! `Origin`, `X-Requested-With`), is refused (`plugin_secret_target_not_allowed`): an address
//! ends up in logs and proxies, and a body is an echo waiting to happen. The request has passed
//! the target check before any marker is filled, so a marker sent anywhere but the address the
//! person confirmed is never filled. Every value is masked out of the plugin's log lines and its
//! answers (`[redacted]`, values of four characters and more), so a server echoing it hands it
//! to nobody.

use std::{borrow::Cow, sync::Arc};

use rc_plugin_api::inventory::SECRET_MARKER_OPEN;
use rc_plugin_types::{Failure, FailureKind};
use secrecy::{ExposeSecret as _, SecretString};
use zeroize::Zeroizing;

use crate::failure::{self, codes};

/// The values a store's markers are filled with, by slot.
pub(crate) type Slots = Arc<[(String, SecretString)]>;

/// What a marker looks like once an address parser percent-encoded its braces.
const ENCODED_OPEN: &str = "%7B%7BSECRET";
/// Headers that travel to third parties: no marker is filled into them.
pub(super) const NO_SECRET_HEADERS: [&str; 3] = ["referer", "origin", "x-requested-with"];
/// What a masked value reads as.
pub(crate) const MASK: &str = "[redacted]";
/// Shorter values are not masked: masking `ab` would blank half of every line.
const MIN_MASKED_CHARS: usize = 4;

/// Whether `text` names a marker, also percent-encoded.
pub(super) fn named(text: &str) -> bool {
    text.contains(SECRET_MARKER_OPEN) || text.to_ascii_uppercase().contains(ENCODED_OPEN)
}

/// Whether `bytes` name a marker.
pub(super) fn named_in(bytes: &[u8]) -> bool {
    bytes
        .windows(SECRET_MARKER_OPEN.len())
        .any(|window| window == SECRET_MARKER_OPEN.as_bytes())
}

pub(super) fn not_allowed() -> Failure {
    failure::coded(
        FailureKind::AccessDenied,
        codes::PLUGIN_SECRET_TARGET_NOT_ALLOWED,
    )
}

fn missing() -> Failure {
    failure::coded(FailureKind::NotFound, codes::PLUGIN_SECRET_MISSING)
}

/// `value` with every `{{secret:<slot>}}` replaced by that slot's value. A slot the store holds
/// no value for, or a marker that does not close, is `plugin_secret_missing` - never an empty
/// string, which would reach the server as a failed sign-in. The result is wiped when dropped,
/// and its buffer is large enough from the start, so no copy is left behind by growing.
pub(super) fn fill(
    value: &str,
    slots: &[(String, SecretString)],
) -> Result<Zeroizing<String>, Failure> {
    let markers = value.matches(SECRET_MARKER_OPEN).count();
    let longest = slots
        .iter()
        .map(|(_, secret)| secret.expose_secret().len())
        .max()
        .unwrap_or(0);
    let mut filled = Zeroizing::new(String::with_capacity(value.len() + markers * longest));
    let mut rest = value;
    while let Some(start) = rest.find(SECRET_MARKER_OPEN) {
        filled.push_str(&rest[..start]);
        let after = &rest[start + SECRET_MARKER_OPEN.len()..];
        let end = after.find("}}").ok_or_else(missing)?;
        let slot = &after[..end];
        let (_, secret) = slots
            .iter()
            .find(|(name, _)| name == slot)
            .ok_or_else(missing)?;
        filled.push_str(secret.expose_secret());
        rest = &after[end + 2..];
    }
    filled.push_str(rest);
    Ok(filled)
}

/// `text` with every stored value of four characters or more replaced by [`MASK`].
pub(crate) fn mask<'a>(text: &'a str, slots: &[(String, SecretString)]) -> Cow<'a, str> {
    let mut masked = Cow::Borrowed(text);
    for (_, secret) in slots {
        let value = secret.expose_secret();
        if value.chars().count() >= MIN_MASKED_CHARS && masked.contains(value) {
            masked = Cow::Owned(masked.replace(value, MASK));
        }
    }
    masked
}

#[cfg(test)]
#[path = "markers_tests.rs"]
mod tests;
