//! A private key taken into the vault (RC-050-03): what can be stored, how it is checked, and how
//! it is opened again when an SSH session needs it.
//!
//! The key file's text is untrusted input. It is checked in two steps: first the armour, the
//! alphabet and the size ([`check_format`], cheap, before any parser sees it), then a full decode
//! with `russh`'s key reader, which proves the key and its passphrase belong together and yields
//! the public half for display ([`inspect`]). Formats: OpenSSH, PEM (PKCS#1 RSA, SEC1 EC), PKCS#8
//! plain or encrypted, and PuTTY `.ppk` versions 2 and 3, each optionally encrypted.
//!
//! The decoded [`PrivateKey`] zeroizes itself on drop; nothing here writes it anywhere, logs it
//! or prints it. [`KeyInfo`] is the public half only.
//!
//! What opening an encrypted key costs is bounded before it starts (RC-080-12): an OpenSSH key's
//! bcrypt rounds and a PuTTY v3 key's argon2 memory, passes and lanes come from the file, and a
//! hostile one could ask for billions of rounds or terabytes; one over the bounds is refused as
//! [`PrivateKeyError::Unknown`].

use russh::keys::{PrivateKey, decode_secret_key, ssh_key};
use serde::{Deserialize, Serialize};

use crate::OfferedKey;

/// The largest key file taken in, in bytes; an 8192-bit RSA key in PEM is under 7 KiB.
pub const MAX_KEY_BYTES: usize = 64 * 1024;

/// The most bcrypt-pbkdf rounds an OpenSSH key may ask for; `ssh-keygen` uses 16 by default.
pub const MAX_BCRYPT_ROUNDS: u32 = 1024;

/// The most memory, in KiB, a PuTTY v3 key's argon2 may ask for: 1 GiB (PuTTYgen uses 8 MiB).
pub const MAX_ARGON2_MEMORY_KIB: u64 = 1024 * 1024;

/// The most memory times passes, in KiB, a PuTTY v3 key's argon2 may ask for: 8 MiB over 4096
/// passes.
pub const MAX_ARGON2_WORK_KIB: u64 = 8 * 1024 * 4096;

/// The most argon2 lanes a PuTTY v3 key may ask for.
pub const MAX_ARGON2_LANES: u64 = 64;

/// How PuTTY starts a key file, followed by the format version.
const PPK_PREFIX: &str = crate::key_markers::PPK_HEADER;

/// The PEM labels of a private key the key reader understands, with their format.
const LABELS: &[(&str, KeyFormat)] = &[
    ("OPENSSH PRIVATE KEY", KeyFormat::OpenSsh),
    ("RSA PRIVATE KEY", KeyFormat::Pem),
    ("EC PRIVATE KEY", KeyFormat::Pem),
    ("PRIVATE KEY", KeyFormat::Pkcs8),
    ("ENCRYPTED PRIVATE KEY", KeyFormat::Pkcs8),
];

/// The format of a private key file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyFormat {
    /// OpenSSH's own format, what `ssh-keygen` writes today.
    OpenSsh,
    /// A traditional PEM key (PKCS#1 RSA, SEC1 EC), optionally encrypted (`Proc-Type`).
    Pem,
    /// PKCS#8, plain or encrypted.
    Pkcs8,
    /// PuTTY's `.ppk`, version 2 or 3.
    Ppk,
}

/// Why a key file cannot be taken in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PrivateKeyError {
    /// Not one private key in a known format, damaged, or larger than [`MAX_KEY_BYTES`].
    #[error("not an OpenSSH, PEM, PKCS#8 or PuTTY private key")]
    Unknown,
    /// The key is encrypted and no passphrase was given.
    #[error("the private key is encrypted; its passphrase is needed")]
    PassphraseNeeded,
    /// The passphrase does not open the key.
    #[error("the passphrase does not open this private key")]
    PassphraseWrong,
}

/// The public half of a private key, kept in clear next to the encrypted key for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyInfo {
    pub format: KeyFormat,
    /// Whether the key file needs a passphrase.
    pub encrypted: bool,
    /// The key type as SSH names it (`ssh-ed25519`, `ssh-rsa`, ...).
    pub algorithm: String,
    /// `SHA256:<base64>` exactly as `ssh-keygen -lf` prints it.
    pub fingerprint: String,
    /// The key's comment, when it has one (OpenSSH and PuTTY keep one, PEM does not).
    pub comment: Option<String>,
    /// The public key as an `authorized_keys` line, without the comment.
    pub public_key: String,
}

/// The format of the key file `text`, from its armour, alphabet and size alone.
///
/// # Errors
///
/// [`PrivateKeyError::Unknown`] for anything that is not one armoured private key or one PuTTY
/// key of version 2 or 3: a public key, a certificate, a damaged file, more than
/// [`MAX_KEY_BYTES`].
pub fn check_format(text: &str) -> Result<KeyFormat, PrivateKeyError> {
    let text = text.trim();
    if text.len() > MAX_KEY_BYTES {
        return Err(PrivateKeyError::Unknown);
    }
    if let Some(rest) = text.strip_prefix(PPK_PREFIX) {
        return if rest.starts_with("2: ") || rest.starts_with("3: ") {
            Ok(KeyFormat::Ppk)
        } else {
            Err(PrivateKeyError::Unknown)
        };
    }
    let mut lines = text.lines().map(str::trim);
    let label = lines
        .next()
        .and_then(|line| line.strip_prefix("-----BEGIN "))
        .and_then(|line| line.strip_suffix("-----"))
        .ok_or(PrivateKeyError::Unknown)?;
    let format = LABELS
        .iter()
        .find(|(known, _)| *known == label)
        .map(|(_, format)| *format)
        .ok_or(PrivateKeyError::Unknown)?;
    let body: Vec<&str> = lines.collect();
    let Some((last, body)) = body.split_last() else {
        return Err(PrivateKeyError::Unknown);
    };
    if *last != format!("-----END {label}-----") {
        return Err(PrivateKeyError::Unknown);
    }
    let mut data = false;
    for line in body {
        if line.is_empty() || is_header(line) {
            continue;
        }
        if !line.bytes().all(is_base64) {
            return Err(PrivateKeyError::Unknown);
        }
        data = true;
    }
    if data {
        Ok(format)
    } else {
        Err(PrivateKeyError::Unknown)
    }
}

/// Opens the key file `text` with `passphrase` (ignored for a key that has none) and describes
/// its public half. CPU-bound for an encrypted key (bcrypt, argon2): call it off the async
/// threads.
///
/// # Errors
///
/// [`PrivateKeyError::Unknown`], [`PrivateKeyError::PassphraseNeeded`] or
/// [`PrivateKeyError::PassphraseWrong`].
pub fn inspect(text: &str, passphrase: Option<&str>) -> Result<KeyInfo, PrivateKeyError> {
    let (format, encrypted, key) = open(text, passphrase)?;
    let offered =
        OfferedKey::from_public_key(key.public_key()).map_err(|_| PrivateKeyError::Unknown)?;
    let comment = key.comment().as_str_lossy().trim().to_owned();
    Ok(KeyInfo {
        format,
        encrypted,
        public_key: format!("{} {}", offered.algorithm, offered.public_key),
        algorithm: offered.algorithm,
        fingerprint: offered.fingerprint,
        comment: (!comment.is_empty()).then_some(comment),
    })
}

/// The key of `text`, opened with `passphrase`, for an SSH session to authenticate with. It
/// lives in memory only and zeroizes itself on drop. CPU-bound like [`inspect`].
///
/// # Errors
///
/// As [`inspect`].
pub fn decode(text: &str, passphrase: Option<&str>) -> Result<PrivateKey, PrivateKeyError> {
    open(text, passphrase).map(|(_, _, key)| key)
}

/// The format, whether it is encrypted, and the opened key.
fn open(
    text: &str,
    passphrase: Option<&str>,
) -> Result<(KeyFormat, bool, PrivateKey), PrivateKeyError> {
    let format = check_format(text)?;
    if format == KeyFormat::OpenSsh {
        let (encrypted, key) = open_openssh(text, passphrase)?;
        return Ok((format, encrypted, key));
    }
    let encrypted = is_encrypted(text, format);
    if encrypted && !kdf_within_bounds(text, format) {
        return Err(PrivateKeyError::Unknown);
    }
    let passphrase = if encrypted {
        Some(passphrase.ok_or(PrivateKeyError::PassphraseNeeded)?)
    } else {
        None
    };
    let key = decode_secret_key(text.trim(), passphrase).map_err(|_| {
        if encrypted {
            PrivateKeyError::PassphraseWrong
        } else {
            PrivateKeyError::Unknown
        }
    })?;
    Ok((format, encrypted, key))
}

/// An OpenSSH key read once (RC-080-51): whether it is encrypted, its bcrypt rounds within the
/// bound, and the key, opened with `passphrase` when it needs one.
fn open_openssh(
    text: &str,
    passphrase: Option<&str>,
) -> Result<(bool, PrivateKey), PrivateKeyError> {
    let Ok(key) = ssh_key::PrivateKey::from_openssh(text.trim()) else {
        // What the strict reader refuses russh's own may still read, as an unencrypted key.
        let key = decode_secret_key(text.trim(), None).map_err(|_| PrivateKeyError::Unknown)?;
        return Ok((false, key));
    };
    if !key.is_encrypted() {
        return Ok((false, key));
    }
    if let ssh_key::Kdf::Bcrypt { rounds, .. } = key.kdf()
        && *rounds > MAX_BCRYPT_ROUNDS
    {
        return Err(PrivateKeyError::Unknown);
    }
    let passphrase = passphrase.ok_or(PrivateKeyError::PassphraseNeeded)?;
    let key = key
        .decrypt(passphrase)
        .map_err(|_| PrivateKeyError::PassphraseWrong)?;
    Ok((true, key))
}

/// Whether the key derivation an encrypted key file of `format` asks for stays within the bounds
/// above, read from its clear parts. PEM's key derivation has no parameters; an encrypted
/// PKCS#8 key's PBES2 parameters are not read here (`docs/security/remote.md`, R-KDF). An
/// OpenSSH key is checked where it is read ([`open_openssh`]).
fn kdf_within_bounds(text: &str, format: KeyFormat) -> bool {
    match format {
        KeyFormat::Ppk => ppk_within_bounds(text),
        KeyFormat::OpenSsh | KeyFormat::Pem | KeyFormat::Pkcs8 => true,
    }
}

/// A PuTTY key's argon2 parameters within the bounds; a v2 key names none (one SHA-1 round).
fn ppk_within_bounds(text: &str) -> bool {
    let number = |name: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(name))
            .map(|value| value.trim().parse::<u64>().ok())
    };
    match (
        number("Argon2-Memory:"),
        number("Argon2-Passes:"),
        number("Argon2-Parallelism:"),
    ) {
        (None, None, None) => true,
        (Some(Some(memory)), Some(Some(passes)), Some(Some(lanes))) => {
            memory <= MAX_ARGON2_MEMORY_KIB
                && memory.saturating_mul(passes) <= MAX_ARGON2_WORK_KIB
                && lanes <= MAX_ARGON2_LANES
        }
        _ => false,
    }
}

/// Whether the key file of `format` needs a passphrase, read from its clear parts; an OpenSSH
/// key's is read with the key ([`open_openssh`]).
fn is_encrypted(text: &str, format: KeyFormat) -> bool {
    match format {
        KeyFormat::Ppk => text
            .lines()
            .filter_map(|line| line.trim().strip_prefix("Encryption:"))
            .any(|value| value.trim() != "none"),
        KeyFormat::Pem => text.contains("Proc-Type: 4,ENCRYPTED"),
        KeyFormat::Pkcs8 => text.contains("-----BEGIN ENCRYPTED PRIVATE KEY-----"),
        KeyFormat::OpenSsh => false,
    }
}

/// A PEM header line of an encrypted PKCS#1 key (`Proc-Type: 4,ENCRYPTED`, `DEK-Info: ...`).
fn is_header(line: &str) -> bool {
    line.split_once(": ").is_some_and(|(name, _)| {
        !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

fn is_base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
}

#[cfg(test)]
#[path = "private_key_tests.rs"]
mod tests;
