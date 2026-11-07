//! One-time codes for a second login factor (RC-060-08): TOTP after RFC 6238, made from a seed
//! the person stored in the vault.
//!
//! A seed is entered as an `otpauth://totp/...` URI (what an authenticator's QR code holds) or
//! as its bare Base32 secret; [`TotpSeed::parse`] checks either and refuses what is not one, with
//! a message that never quotes the secret. The key lives zeroized on drop and never reaches a
//! `Debug` line. [`TotpSeed::code_at`] is the code of one moment: HMAC-SHA1, -SHA256 or -SHA512
//! over the time step, dynamically truncated, six to eight digits - with the `hmac`, `sha1` and
//! `sha2` crates the workspace already has. The code is a secret too, as long as it is valid: it
//! comes back as a [`SecretString`].
//!
//! [`DEFAULT_PROMPT`] is the pattern a `keyboard-interactive` question is taken for a code by,
//! unless a connection sets its own.

mod base32;
mod uri;

use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use hmac::{Hmac, KeyInit, Mac};
use secrecy::{
    ExposeSecret as _, SecretSlice, SecretString,
    zeroize::{Zeroize as _, Zeroizing},
};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

/// The pattern a server's question is taken for a code by: Google Authenticator's PAM module
/// asks "Verification code:", others "One-time password", "OTP" or "Token".
pub const DEFAULT_PROMPT: &str = "(?i)verification code|one-time|otp|token";

/// Characters a seed's text holds at most, URI included.
pub const MAX_SEED_CHARS: usize = 2048;

/// Bytes a decoded key holds at least: 80 bits, the shortest seed services hand out.
pub const MIN_KEY_BYTES: usize = 10;

/// Bytes a decoded key holds at most.
pub const MAX_KEY_BYTES: usize = 128;

/// The digits a code may have.
pub const DIGITS: std::ops::RangeInclusive<u32> = 6..=8;

/// The seconds one code may stand for.
pub const PERIOD_SECS: std::ops::RangeInclusive<u64> = 1..=300;

/// The stable code of every refusal, as the engine answers it.
pub const TOTP_SEED_INVALID: &str = "totp_seed_invalid";

/// The HMAC a code is made with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TotpAlgorithm {
    #[default]
    Sha1,
    Sha256,
    Sha512,
}

impl TotpAlgorithm {
    /// The algorithm an `otpauth` URI names (`SHA1`, `SHA256`, `SHA512`, any case).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_uppercase().as_str() {
            "SHA1" => Some(Self::Sha1),
            "SHA256" => Some(Self::Sha256),
            "SHA512" => Some(Self::Sha512),
            _ => None,
        }
    }
}

/// Why a text is no seed. The messages name what is wrong, never the secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TotpError {
    #[error("a one-time code seed must not be empty")]
    Empty,
    #[error("a one-time code seed holds at most {max} characters")]
    TooLong { max: usize },
    #[error("an otpauth URI for one-time codes starts with otpauth://totp/")]
    NotTotp,
    #[error("the otpauth URI is malformed: {reason}")]
    Malformed { reason: &'static str },
    #[error("the otpauth URI has no secret")]
    SecretMissing,
    #[error("the secret is no Base32 text")]
    NotBase32,
    #[error("the secret holds {MIN_KEY_BYTES} to {MAX_KEY_BYTES} bytes")]
    KeyLength,
    #[error("the algorithm is SHA1, SHA256 or SHA512")]
    AlgorithmUnknown,
    #[error("a code has 6 to 8 digits")]
    DigitsInvalid,
    #[error("a code stands for 1 to 300 seconds")]
    PeriodInvalid,
}

impl TotpError {
    /// The stable code: one for every refusal.
    #[must_use]
    pub fn code(&self) -> &'static str {
        TOTP_SEED_INVALID
    }
}

/// A checked seed: the key, the algorithm, the digits and the period.
pub struct TotpSeed {
    key: SecretSlice<u8>,
    algorithm: TotpAlgorithm,
    digits: u32,
    period: u64,
}

impl TotpSeed {
    /// `text` as a seed: an `otpauth://totp/...` URI, or a bare Base32 secret (SHA-1, six
    /// digits, 30 seconds). Spaces, dashes and `=` padding in a bare secret are ignored, as
    /// authenticators print it in groups.
    ///
    /// # Errors
    ///
    /// Empty, too long, another URI, a malformed one, no or no valid secret, or an algorithm,
    /// digit count or period outside the bounds.
    pub fn parse(text: &str) -> Result<Self, TotpError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(TotpError::Empty);
        }
        if text.chars().count() > MAX_SEED_CHARS {
            return Err(TotpError::TooLong {
                max: MAX_SEED_CHARS,
            });
        }
        let has_scheme = text
            .get(..uri::SCHEME.len())
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case(uri::SCHEME));
        if has_scheme {
            return uri::parse(text);
        }
        if text.contains("://") {
            return Err(TotpError::NotTotp);
        }
        Self::new(text, TotpAlgorithm::Sha1, 6, 30)
    }

    /// A seed from its Base32 `secret` and its parameters, each checked.
    fn new(
        secret: &str,
        algorithm: TotpAlgorithm,
        digits: u32,
        period: u64,
    ) -> Result<Self, TotpError> {
        if !DIGITS.contains(&digits) {
            return Err(TotpError::DigitsInvalid);
        }
        if !PERIOD_SECS.contains(&period) {
            return Err(TotpError::PeriodInvalid);
        }
        let key = base32::decode(secret).ok_or(TotpError::NotBase32)?;
        if !(MIN_KEY_BYTES..=MAX_KEY_BYTES).contains(&key.len()) {
            return Err(TotpError::KeyLength);
        }
        Ok(Self {
            key: SecretSlice::from(key.to_vec()),
            algorithm,
            digits,
            period,
        })
    }

    #[must_use]
    pub fn algorithm(&self) -> TotpAlgorithm {
        self.algorithm
    }

    #[must_use]
    pub fn digits(&self) -> u32 {
        self.digits
    }

    /// The seconds one code stands for.
    #[must_use]
    pub fn period(&self) -> u64 {
        self.period
    }

    /// The code for the moment `unix_secs` seconds after 1970-01-01 UTC.
    #[must_use]
    pub fn code_at(&self, unix_secs: u64) -> SecretString {
        let counter = (unix_secs / self.period).to_be_bytes();
        let key = self.key.expose_secret();
        let hash = match self.algorithm {
            TotpAlgorithm::Sha1 => sign::<Hmac<Sha1>>(key, &counter),
            TotpAlgorithm::Sha256 => sign::<Hmac<Sha256>>(key, &counter),
            TotpAlgorithm::Sha512 => sign::<Hmac<Sha512>>(key, &counter),
        };
        let value = truncate(&hash) % 10_u64.pow(self.digits);
        let width = usize::try_from(self.digits).unwrap_or(6);
        let mut text = format!("{value:0width$}");
        let code = SecretString::from(text.as_str());
        text.zeroize();
        code
    }

    /// The code for now.
    #[must_use]
    pub fn code_now(&self) -> SecretString {
        self.code_at(unix_now())
    }
}

impl fmt::Debug for TotpSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TotpSeed")
            .field("key", &"[redacted]")
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("period", &self.period)
            .finish()
    }
}

/// Seconds since 1970-01-01 UTC; 0 for a clock set before it.
#[must_use]
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

/// The HMAC of `message` under `key`.
fn sign<M: Mac + KeyInit>(key: &[u8], message: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut mac = <M as KeyInit>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    Zeroizing::new(mac.finalize().into_bytes().to_vec())
}

/// RFC 4226's dynamic truncation: 31 bits from the offset the last nibble names.
fn truncate(hash: &[u8]) -> u64 {
    let offset = hash.last().map_or(0, |last| usize::from(last & 0x0f));
    let word = hash
        .get(offset..offset + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map_or(0, u32::from_be_bytes);
    u64::from(word & 0x7fff_ffff)
}

#[cfg(test)]
#[path = "totp_tests.rs"]
mod tests;
