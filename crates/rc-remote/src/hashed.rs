//! Hashed `known_hosts` names (`HashKnownHosts yes`): `|1|<salt>|<hash>`, both base64, where
//! the hash is HMAC-SHA1 keyed with the salt over the name (`host` or `[host]:port`). A hashed
//! name cannot be read back, only checked against a name in question.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use rc_db::HASHED_PREFIX;
use sha1::Sha1;

/// Whether `hashed` (`|1|salt|hash`) is the hash of `name`. A malformed hashed name matches
/// nothing.
#[must_use]
pub(crate) fn matches(hashed: &str, name: &str) -> bool {
    let Some((salt, hash)) = parts(hashed) else {
        return false;
    };
    let Ok(mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.chain_update(name.as_bytes())
        .verify_slice(&hash)
        .is_ok()
}

/// Whether `hashed` is a well-formed hashed name: the prefix, a salt and a 20-byte hash.
#[must_use]
pub(crate) fn is_well_formed(hashed: &str) -> bool {
    parts(hashed).is_some_and(|(_, hash)| hash.len() == 20)
}

/// The decoded salt and hash.
fn parts(hashed: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let (salt, hash) = hashed.strip_prefix(HASHED_PREFIX)?.split_once('|')?;
    Some((STANDARD.decode(salt).ok()?, STANDARD.decode(hash).ok()?))
}

#[cfg(test)]
#[path = "hashed_tests.rs"]
mod tests;
