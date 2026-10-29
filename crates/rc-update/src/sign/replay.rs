//! Freshness: refusing a correctly signed document that is simply old.
//!
//! A signature says who wrote a document, never when; without a freshness rule whoever answers
//! the update request replays yesterday's genuine manifest for as long as they like, and the
//! installation never learns that a fix exists. Three rules: the sequence never goes backwards,
//! `not_after` bounds how long a replay stays useful on a first contact, and an `issued_at` far in
//! the future is refused.
//!
//! Copied from rDownloader's `crates/rd-sign/src/replay.rs` at commit 26c8592a6 (RC-100-04).

use chrono::{DateTime, Duration, Utc};

/// How far ahead of local time an `issued_at` may sit before it stops being clock skew.
pub(crate) const MAX_CLOCK_SKEW_MINUTES: i64 = 60;

/// The freshness fields every signed document carries.
#[derive(Clone, Copy, Debug)]
pub struct Freshness {
    /// The publisher's monotonic counter for this document kind.
    pub sequence: u64,
    /// When the publisher says it signed this.
    pub issued_at: DateTime<Utc>,
    /// After this instant the document is stale even if nothing newer has been seen.
    pub not_after: Option<DateTime<Utc>>,
}

/// Why a document was refused as stale.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StaleError {
    #[error("document sequence {saw} is not newer than the {known} already accepted")]
    Replayed { saw: u64, known: u64 },
    #[error("document expired at {expired_at}")]
    Expired { expired_at: DateTime<Utc> },
    #[error("document is issued {issued_at}, further ahead than clock skew explains")]
    FromTheFuture { issued_at: DateTime<Utc> },
}

/// Checks `candidate` against the highest sequence already accepted, if any; `known` is `None`
/// on a first contact, where only expiry and the skew bound apply.
///
/// # Errors
///
/// See [`StaleError`].
pub fn check(
    candidate: Freshness,
    known: Option<u64>,
    now: DateTime<Utc>,
) -> Result<(), StaleError> {
    if let Some(known) = known
        && candidate.sequence <= known
    {
        return Err(StaleError::Replayed {
            saw: candidate.sequence,
            known,
        });
    }
    if let Some(expired_at) = candidate.not_after
        && now > expired_at
    {
        return Err(StaleError::Expired { expired_at });
    }
    if candidate.issued_at > now + Duration::minutes(MAX_CLOCK_SKEW_MINUTES) {
        return Err(StaleError::FromTheFuture {
            issued_at: candidate.issued_at,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod tests;
