//! The trust store's operations: the verdict on a presented key against `rcommander.db`, and
//! the three ways a key gets in - a person trusts a first sighting, a person replaces a changed
//! key by its own action, or an import. A key a `known_hosts` file revoked gets in by none of
//! them, and its verdict is [`Verdict::Revoked`] whatever is stored for the host (RC-080-67).
//!
//! [`trust`] is the TOFU dialog's "Trust and save" and only ever stores a first sighting: for
//! a key that changed it fails with [`TrustError::Changed`], whatever the caller intended.
//! Replacing a stored key is [`replace`], which names the stored key it replaces.

use rc_db::{Db, DbError, KnownHost, KnownHostSource, KnownHostsImport, NewKnownHost};

use crate::{
    hostkey::{OfferedKey, Verdict, decide, normalize_host},
    known_hosts_import::ParsedKnownHosts,
};

/// Why a key was not stored.
#[derive(Debug, thiserror::Error)]
pub enum TrustError {
    /// Another key of this type is stored for the host: trusting is for first sightings only.
    #[error("{host}:{port} has another {algorithm} key stored; replace it explicitly")]
    Changed {
        host: String,
        port: u16,
        algorithm: String,
        stored_fingerprint: String,
    },
    /// The stored key is not the one the replacement names any more.
    #[error("the stored key changed since it was shown")]
    Stale,
    /// The key is revoked: it is never trusted.
    #[error("the key {fingerprint} is revoked")]
    Revoked { fingerprint: String },
    #[error(transparent)]
    Db(#[from] DbError),
}

/// The verdict on `offered` for `host` on `port`. A revoked key is that before anything else; a
/// trusted key's `last_seen_at` is updated.
///
/// # Errors
///
/// The database fails.
pub async fn verify(
    db: &Db,
    host: &str,
    port: u16,
    offered: OfferedKey,
) -> Result<Verdict, DbError> {
    if let Some(revoked) = db.revoked_host_key(&offered.fingerprint).await? {
        return Ok(Verdict::Revoked { revoked, offered });
    }
    let candidates = db
        .known_host_candidates(&normalize_host(host), port)
        .await?;
    let verdict = decide(&candidates, host, port, offered);
    if let Verdict::Trusted(stored) = &verdict {
        db.touch_known_host(stored.id).await?;
    }
    Ok(verdict)
}

/// The key types stored for `host` on `port` ([`crate::hostkey::stored_types`]).
///
/// # Errors
///
/// The database fails.
pub async fn stored_types(db: &Db, host: &str, port: u16) -> Result<Vec<String>, DbError> {
    let candidates = db
        .known_host_candidates(&normalize_host(host), port)
        .await?;
    Ok(crate::hostkey::stored_types(&candidates, host, port))
}

/// Stores a first sighting a person confirmed. A key already trusted is answered as stored.
///
/// # Errors
///
/// [`TrustError::Changed`] when another key of this type is stored for the host - never
/// overwritten here -, [`TrustError::Revoked`] for a revoked key, and database failures.
pub async fn trust(
    db: &Db,
    host: &str,
    port: u16,
    offered: OfferedKey,
) -> Result<KnownHost, TrustError> {
    match verify(db, host, port, offered.clone()).await? {
        Verdict::Trusted(stored) => return Ok(stored),
        Verdict::Changed { stored, .. } => return Err(changed(host, port, &stored)),
        Verdict::Revoked { offered, .. } => return Err(revoked(&offered)),
        Verdict::Unknown(_) => {}
    }
    let new = NewKnownHost {
        host: normalize_host(host),
        port,
        algorithm: offered.algorithm.clone(),
        public_key: offered.public_key.clone(),
        fingerprint_sha256: offered.fingerprint.clone(),
        source: KnownHostSource::User,
    };
    if let Some(stored) = db.add_known_host(&new).await? {
        return Ok(stored);
    }
    // Stored in between by someone else: judge again, never overwrite.
    match verify(db, host, port, offered).await? {
        Verdict::Trusted(stored) => Ok(stored),
        Verdict::Changed { stored, .. } => Err(changed(host, port, &stored)),
        Verdict::Revoked { offered, .. } => Err(revoked(&offered)),
        Verdict::Unknown(_) => Err(TrustError::Stale),
    }
}

/// Replaces the stored key `stored` (as a person was shown it) with `offered` for `host` on
/// `port`: the explicit action for a changed key, never part of the first-sighting dialog.
///
/// # Errors
///
/// [`TrustError::Stale`] when `stored` is gone or carries another key by now,
/// [`TrustError::Revoked`] when `offered` is revoked, and database failures.
pub async fn replace(
    db: &Db,
    stored: &KnownHost,
    host: &str,
    port: u16,
    offered: &OfferedKey,
) -> Result<KnownHost, TrustError> {
    if db.revoked_host_key(&offered.fingerprint).await?.is_some() {
        return Err(revoked(offered));
    }
    let new = NewKnownHost {
        host: normalize_host(host),
        port,
        algorithm: offered.algorithm.clone(),
        public_key: offered.public_key.clone(),
        fingerprint_sha256: offered.fingerprint.clone(),
        source: KnownHostSource::User,
    };
    db.replace_known_host(stored.id, &stored.fingerprint_sha256, &new)
        .await?
        .ok_or(TrustError::Stale)
}

/// What [`import`] stored: the trusted entries, and how many revoked keys are new.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imported {
    pub keys: KnownHostsImport,
    pub revoked: usize,
}

/// Stores the entries of a parsed `known_hosts` file - conflicting keys stay as they are - and
/// its revoked keys, those first.
///
/// # Errors
///
/// The database fails; the step that failed stores nothing.
pub async fn import(db: &Db, parsed: &ParsedKnownHosts) -> Result<Imported, DbError> {
    let revoked = db.add_revoked_host_keys(&parsed.revoked).await?;
    let keys = db.import_known_hosts(&parsed.entries).await?;
    Ok(Imported { keys, revoked })
}

fn revoked(offered: &OfferedKey) -> TrustError {
    TrustError::Revoked {
        fingerprint: offered.fingerprint.clone(),
    }
}

fn changed(host: &str, port: u16, stored: &KnownHost) -> TrustError {
    TrustError::Changed {
        host: normalize_host(host),
        port,
        algorithm: stored.algorithm.clone(),
        stored_fingerprint: stored.fingerprint_sha256.clone(),
    }
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
