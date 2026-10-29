//! Server host-key verification, after rDownloader's `rd-sftp/src/hostkey.rs`.
//!
//! An SSH client that accepts every host key is indistinguishable from one talking to an
//! attacker, and one that accepts none is unusable. The middle ground is a trust store: a key is
//! confirmed once by a person, and any later change is surfaced rather than resolved
//! automatically.
//!
//! The decision deliberately does **not** live in the `russh` handler callback. Returning
//! `false` there produces an opaque protocol error that cannot say *why* the connection was
//! refused, so the handler records what it saw ([`describe`]) and the caller turns the
//! [`Verdict`] into a coded failure carrying the fingerprint a person has to confirm
//! ([`HOST_KEY_UNKNOWN`], [`HOST_KEY_CHANGED`]).

use base64::{Engine as _, engine::general_purpose::STANDARD};
use rc_db::KnownHost;
use russh::keys::{Algorithm, HashAlg, PublicKey, PublicKeyOrCertificate};
use serde::{Deserialize, Serialize};

use crate::hashed;

/// Connection refused because the server is not in the trust store yet.
pub const HOST_KEY_UNKNOWN: &str = "host_key_unknown";
/// Connection refused because the server presented a different key than the stored one.
pub const HOST_KEY_CHANGED: &str = "host_key_changed";
/// The server offered a certificate; certificate-based host trust is not implemented.
pub const HOST_KEY_UNSUPPORTED: &str = "host_key_unsupported";
/// A key that does not parse as an SSH public key.
pub const HOST_KEY_INVALID: &str = "host_key_invalid";

/// The SSH port, which `known_hosts` leaves out of a name.
pub const DEFAULT_PORT: u16 = 22;

/// A key a server presented (or a `known_hosts` line names), in the forms a person compares
/// and the trust store keeps.
///
/// Nothing here is secret: it is the public half and its digest, exactly what a person has to
/// compare against the server's, so it travels in errors, events and MCP answers unmasked.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OfferedKey {
    /// The key type as SSH names it (`ssh-ed25519`, `ecdsa-sha2-nistp256`, `ssh-rsa`, ...).
    pub algorithm: String,
    /// `SHA256:<base64>` exactly as `ssh-keygen -lf` prints it.
    pub fingerprint: String,
    /// The key's wire blob in base64, as `known_hosts` spells it.
    pub public_key: String,
}

/// Why a key cannot be judged.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum KeyError {
    /// A certificate, which this trust store has no way to pin.
    #[error("the SSH server authenticates with a certificate, which is not supported")]
    Certificate,
    /// Not a public key SSH can read.
    #[error("not a valid SSH public key")]
    Invalid,
}

impl KeyError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[HOST_KEY_UNSUPPORTED, HOST_KEY_INVALID];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Certificate => HOST_KEY_UNSUPPORTED,
            Self::Invalid => HOST_KEY_INVALID,
        }
    }
}

impl OfferedKey {
    /// The forms of a parsed key.
    ///
    /// # Errors
    ///
    /// [`KeyError::Invalid`] when the key does not encode.
    pub fn from_public_key(key: &PublicKey) -> Result<Self, KeyError> {
        let blob = key.to_bytes().map_err(|_| KeyError::Invalid)?;
        Ok(Self {
            algorithm: key.algorithm().as_str().to_owned(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            public_key: STANDARD.encode(blob),
        })
    }

    /// The key of a `known_hosts` line: its type and its base64 blob. The type has to be the
    /// one inside the blob.
    ///
    /// # Errors
    ///
    /// [`KeyError::Invalid`] when the blob is not a key of that type.
    pub fn from_openssh(algorithm: &str, base64: &str) -> Result<Self, KeyError> {
        let key = PublicKey::from_openssh(&format!("{algorithm} {base64}"))
            .map_err(|_| KeyError::Invalid)?;
        Self::from_public_key(&key)
    }

    /// The parsed key again.
    fn parsed(&self) -> Option<PublicKey> {
        let blob = STANDARD.decode(&self.public_key).ok()?;
        PublicKey::from_bytes(&blob).ok()
    }

    /// The key's "randomart" as `ssh-keygen -lv` draws it; `None` when the blob does not
    /// parse.
    #[must_use]
    pub fn randomart(&self) -> Option<String> {
        let key = self.parsed()?;
        Some(
            key.fingerprint(HashAlg::Sha256)
                .to_randomart(&randomart_header(&key)),
        )
    }
}

/// `[ED25519 256]`, `[ECDSA 256]`, `[RSA 3072]`: the box's title in `ssh-keygen -lv`.
fn randomart_header(key: &PublicKey) -> String {
    match key.algorithm() {
        Algorithm::Ed25519 => "[ED25519 256]".to_owned(),
        Algorithm::Ecdsa { curve } => {
            let bits = curve.as_str().trim_start_matches("nistp");
            format!("[ECDSA {bits}]")
        }
        Algorithm::Rsa { .. } => {
            let bits = key.key_data().rsa().map_or(0, |rsa| rsa.key_size());
            format!("[RSA {bits}]")
        }
        other => format!("[{}]", other.as_str()),
    }
}

/// Reads the forms of the key a server offered during key exchange.
///
/// A certificate is refused rather than reduced to its signing key: pinning the CA is a
/// different trust model, and silently pinning the leaf would accept every future certificate
/// the same CA issues.
///
/// # Errors
///
/// [`KeyError::Certificate`] for a certificate, [`KeyError::Invalid`] for a key that does not
/// encode.
pub fn describe(offered: &PublicKeyOrCertificate) -> Result<OfferedKey, KeyError> {
    match offered {
        PublicKeyOrCertificate::PublicKey { key, .. } => OfferedKey::from_public_key(key),
        PublicKeyOrCertificate::Certificate(_) => Err(KeyError::Certificate),
    }
}

/// What the trust store says about a key a server offered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Exactly this key was confirmed (or imported) for this host and port.
    Trusted(KnownHost),
    /// Nothing is stored for this host, port and key type: a first sighting.
    Unknown(OfferedKey),
    /// Another key of this type is stored. A rebuilt server and an interception look identical
    /// from here, so this is never resolved automatically.
    Changed {
        stored: KnownHost,
        offered: OfferedKey,
    },
}

impl Verdict {
    /// The code a connection refused on this verdict fails with; `None` for a trusted key.
    #[must_use]
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Trusted(_) => None,
            Self::Unknown(_) => Some(HOST_KEY_UNKNOWN),
            Self::Changed { .. } => Some(HOST_KEY_CHANGED),
        }
    }
}

/// The host as names are stored and hashed: lower case, without surrounding whitespace.
#[must_use]
pub fn normalize_host(host: &str) -> String {
    host.trim().to_ascii_lowercase()
}

/// The name `known_hosts` uses for a host and port: `host` on port 22, `[host]:port` elsewhere.
/// A hashed name is the HMAC of exactly this text.
#[must_use]
pub fn host_pattern(host: &str, port: u16) -> String {
    let host = normalize_host(host);
    if port == DEFAULT_PORT {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

/// Whether the stored row names `host` on `port`: the same name and port, or a hashed name
/// whose HMAC matches [`host_pattern`].
#[must_use]
pub fn names_host(stored: &KnownHost, host: &str, port: u16) -> bool {
    if stored.is_hashed() {
        hashed::matches(&stored.host, &host_pattern(host, port))
    } else {
        stored.host == normalize_host(host) && stored.port == port
    }
}

/// The key types stored for `host` on `port`, each once, in the order of `candidates`: what the
/// key exchange asks for first, and what the dialog of an unknown key of another type names
/// (RC-080-12).
#[must_use]
pub fn stored_types(candidates: &[KnownHost], host: &str, port: u16) -> Vec<String> {
    let mut types: Vec<String> = Vec::new();
    for stored in candidates
        .iter()
        .filter(|stored| names_host(stored, host, port))
    {
        if !types.contains(&stored.algorithm) {
            types.push(stored.algorithm.clone());
        }
    }
    types
}

/// The host-key algorithms to offer in the key exchange: those signing with a key type in
/// `stored` first, then the rest, each group in the order of `defaults`. A server that has a key
/// of a stored type presents it, rather than another one the trust store would meet as a first
/// sighting (RC-080-12).
#[must_use]
pub fn preferred_key_algorithms(stored: &[String], defaults: &[Algorithm]) -> Vec<Algorithm> {
    let (known, rest): (Vec<Algorithm>, Vec<Algorithm>) = defaults
        .iter()
        .cloned()
        .partition(|algorithm| stored.iter().any(|stored| stored == key_type(algorithm)));
    known.into_iter().chain(rest).collect()
}

/// The key type a host-key algorithm signs with, as keys are stored: every RSA variant is
/// `ssh-rsa`.
fn key_type(algorithm: &Algorithm) -> &str {
    match algorithm {
        Algorithm::Rsa { .. } => "ssh-rsa",
        other => other.as_str(),
    }
}

/// The verdict on `offered` for `host` on `port`, from the stored rows that may belong to it
/// (`rc_db::Db::known_host_candidates`; rows of other hosts are ignored). Keys are compared per
/// key type, as OpenSSH does: a host may have one of each.
#[must_use]
pub fn decide(candidates: &[KnownHost], host: &str, port: u16, offered: OfferedKey) -> Verdict {
    let mut same_type = candidates
        .iter()
        .filter(|stored| stored.algorithm == offered.algorithm && names_host(stored, host, port))
        .peekable();
    let Some(first) = same_type.peek().copied() else {
        return Verdict::Unknown(offered);
    };
    if let Some(trusted) = same_type.find(|stored| stored.fingerprint_sha256 == offered.fingerprint)
    {
        return Verdict::Trusted(trusted.clone());
    }
    Verdict::Changed {
        stored: first.clone(),
        offered,
    }
}

#[cfg(test)]
#[path = "hostkey_tests.rs"]
mod tests;
