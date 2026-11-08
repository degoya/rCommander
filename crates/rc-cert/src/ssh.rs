//! OpenSSH's own formats: public key lines (`id_ed25519.pub`, `authorized_keys` with options) and
//! the `openssh-key-v1` container of a private key, of which only the unencrypted public part is
//! read - the private section, encrypted or not, is never looked into.

use base64::Engine as _;

use crate::der;
use crate::oids::encoded;
use crate::pubkey::{self, PublicKey};
use crate::show;

/// The most keys an `openssh-key-v1` container is taken to hold (OpenSSH writes one).
const MAX_CONTAINER_KEYS: u32 = 16;

/// One public key line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyLine {
    /// The key type the line names (`ssh-ed25519`, `ecdsa-sha2-nistp256`, ...).
    pub kind: String,
    /// The key, for the types taken apart here; `None` for security keys and certificates.
    pub key: Option<PublicKey>,
    /// `SHA256:...` of the blob, as `ssh-keygen -l` shows it.
    pub fingerprint: String,
    /// What follows the key, usually `user@host`.
    pub comment: Option<String>,
    /// What comes before the key in `authorized_keys` (`command="..."`, `no-pty`, ...).
    pub options: Option<String>,
}

/// The public part of an `openssh-key-v1` container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    /// `none`, or the cipher and the key derivation (`aes256-ctr, bcrypt`).
    pub cipher: Option<String>,
    /// One entry per key: its type, its key where taken apart, its fingerprint.
    pub keys: Vec<(String, Option<PublicKey>, String)>,
}

/// Reads one public key line; `None` when it holds no key.
#[must_use]
pub fn key_line(line: &str) -> Option<KeyLine> {
    let tokens = tokens(line);
    for (index, kind) in tokens.iter().enumerate() {
        if !is_key_type(kind) {
            continue;
        }
        let Some(encoded) = tokens.get(index + 1) else {
            continue;
        };
        let Ok(blob) = crate::pem::BASE64.decode(encoded) else {
            continue;
        };
        let mut reader = Reader::new(&blob);
        if reader.string() != Some(kind.as_bytes()) {
            continue;
        }
        let joined = |part: &[String]| (!part.is_empty()).then(|| show::clean(&part.join(" ")));
        return Some(KeyLine {
            kind: show::clean(kind),
            key: from_blob(&blob),
            fingerprint: pubkey::ssh_fingerprint(&blob),
            comment: tokens.get(index + 2..).and_then(joined),
            options: tokens.get(..index).and_then(joined),
        });
    }
    None
}

/// Whether a token names an OpenSSH key type.
fn is_key_type(token: &str) -> bool {
    token.starts_with("ssh-")
        || token.starts_with("ecdsa-sha2-")
        || token.starts_with("sk-ssh-")
        || token.starts_with("sk-ecdsa-")
}

/// A line split at white space outside double quotes, as `authorized_keys` options are.
fn tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' && quoted {
            current.push(character);
            escaped = true;
        } else if character == '"' {
            current.push(character);
            quoted = !quoted;
        } else if character.is_whitespace() && !quoted {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// The public part of an `openssh-key-v1` container (the body of `OPENSSH PRIVATE KEY`).
#[must_use]
pub fn container(body: &[u8]) -> Option<Container> {
    let rest = body.strip_prefix(b"openssh-key-v1\0")?;
    let mut reader = Reader::new(rest);
    let cipher = reader.string()?;
    let kdf = reader.string()?;
    let _kdf_options = reader.string()?;
    let count = reader.u32()?;
    if count == 0 || count > MAX_CONTAINER_KEYS {
        return None;
    }
    let mut keys = Vec::new();
    for _ in 0..count {
        let blob = reader.string()?;
        let kind = Reader::new(blob).string()?;
        keys.push((
            show::clean_bytes(kind),
            from_blob(blob),
            pubkey::ssh_fingerprint(blob),
        ));
    }
    let cipher = (cipher != b"none")
        .then(|| format!("{}, {}", show::clean_bytes(cipher), show::clean_bytes(kdf)));
    Some(Container { cipher, keys })
}

/// The key of an OpenSSH public key blob, for the plain key types.
#[must_use]
pub fn from_blob(blob: &[u8]) -> Option<PublicKey> {
    let mut reader = Reader::new(blob);
    let key = match reader.string()? {
        b"ssh-rsa" => {
            let e = reader.mpint()?;
            let n = reader.mpint()?;
            PublicKey::Rsa { n, e }
        }
        b"ssh-ed25519" => PublicKey::Ed25519(reader.string()?.to_vec()),
        b"ssh-dss" => PublicKey::Dsa {
            p: reader.mpint()?,
            q: reader.mpint()?,
            g: reader.mpint()?,
            y: reader.mpint()?,
        },
        b"ecdsa-sha2-nistp256" | b"ecdsa-sha2-nistp384" | b"ecdsa-sha2-nistp521" => {
            let curve = match reader.string()? {
                b"nistp256" => encoded::P256,
                b"nistp384" => encoded::P384,
                b"nistp521" => encoded::P521,
                _ => return None,
            };
            PublicKey::Ec {
                curve: curve.to_vec(),
                point: reader.string()?.to_vec(),
            }
        }
        _ => return None,
    };
    Some(key)
}

/// SSH's wire encoding, read from the front.
struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.rest.split_first_chunk::<4>()?;
        self.rest = rest;
        Some(u32::from_be_bytes(*head))
    }

    /// A length-prefixed byte string.
    fn string(&mut self) -> Option<&'a [u8]> {
        let length = usize::try_from(self.u32()?).ok()?;
        let bytes = self.rest.get(..length)?;
        self.rest = self.rest.get(length..)?;
        Some(bytes)
    }

    /// A non-negative `mpint` as its magnitude.
    fn mpint(&mut self) -> Option<Vec<u8>> {
        let bytes = self.string()?;
        if bytes.first().is_some_and(|&top| top & 0x80 != 0) {
            return None;
        }
        Some(der::magnitude(bytes).to_vec())
    }
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;
