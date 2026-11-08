//! Object identifiers: the dotted form of an encoded one, and the names of the algorithms,
//! curves and ciphers the viewer shows. An identifier not listed is shown dotted.

/// The encoded identifiers the canonical SubjectPublicKeyInfo writes (`pubkey::PublicKey`).
pub mod encoded {
    pub const RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
    pub const EC: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
    pub const DSA: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x38, 0x04, 0x01];
    pub const ED25519: &[u8] = &[0x2b, 0x65, 0x70];
    pub const ED448: &[u8] = &[0x2b, 0x65, 0x71];
    pub const X25519: &[u8] = &[0x2b, 0x65, 0x6e];
    pub const X448: &[u8] = &[0x2b, 0x65, 0x6f];
    pub const P256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
    pub const P384: &[u8] = &[0x2b, 0x81, 0x04, 0x00, 0x22];
    pub const P521: &[u8] = &[0x2b, 0x81, 0x04, 0x00, 0x23];
    /// PKCS#7 `data`, the content type a PKCS#12 file's authenticated safe starts with.
    pub const PKCS7_DATA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x01];
}

/// The longest encoded identifier read; real ones are a few dozen bytes.
const MAX_ENCODED: usize = 64;

/// The dotted form of an encoded identifier, `None` when it is malformed or too long.
#[must_use]
pub fn dotted(content: &[u8]) -> Option<String> {
    if content.is_empty() || content.len() > MAX_ENCODED {
        return None;
    }
    let mut arcs: Vec<u64> = Vec::new();
    let mut value = 0u64;
    let mut open = false;
    for &byte in content {
        if value > (u64::MAX >> 7) {
            return None;
        }
        value = (value << 7) | u64::from(byte & 0x7f);
        open = byte & 0x80 != 0;
        if !open {
            if arcs.is_empty() {
                let first = (value / 40).min(2);
                arcs.push(first);
                arcs.push(value - first * 40);
            } else {
                arcs.push(value);
            }
            value = 0;
        }
    }
    if open {
        return None;
    }
    Some(
        arcs.iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join("."),
    )
}

/// How far a signature algorithm is trusted today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
    Good,
    /// SHA-1: collisions are practical.
    Weak,
    /// MD2, MD5: broken.
    Broken,
}

/// Signature algorithms: dotted identifier, name, strength.
const SIGNATURES: &[(&str, &str, Strength)] = &[
    (
        "1.2.840.113549.1.1.2",
        "md2WithRSAEncryption",
        Strength::Broken,
    ),
    (
        "1.2.840.113549.1.1.4",
        "md5WithRSAEncryption",
        Strength::Broken,
    ),
    (
        "1.2.840.113549.1.1.5",
        "sha1WithRSAEncryption",
        Strength::Weak,
    ),
    ("1.3.14.3.2.29", "sha1WithRSASignature", Strength::Weak),
    ("1.2.840.113549.1.1.10", "RSASSA-PSS", Strength::Good),
    (
        "1.2.840.113549.1.1.11",
        "sha256WithRSAEncryption",
        Strength::Good,
    ),
    (
        "1.2.840.113549.1.1.12",
        "sha384WithRSAEncryption",
        Strength::Good,
    ),
    (
        "1.2.840.113549.1.1.13",
        "sha512WithRSAEncryption",
        Strength::Good,
    ),
    (
        "1.2.840.113549.1.1.14",
        "sha224WithRSAEncryption",
        Strength::Good,
    ),
    ("1.2.840.10045.4.1", "ecdsa-with-SHA1", Strength::Weak),
    ("1.2.840.10045.4.3.1", "ecdsa-with-SHA224", Strength::Good),
    ("1.2.840.10045.4.3.2", "ecdsa-with-SHA256", Strength::Good),
    ("1.2.840.10045.4.3.3", "ecdsa-with-SHA384", Strength::Good),
    ("1.2.840.10045.4.3.4", "ecdsa-with-SHA512", Strength::Good),
    ("1.2.840.10040.4.3", "dsa-with-SHA1", Strength::Weak),
    ("2.16.840.1.101.3.4.3.1", "dsa-with-SHA224", Strength::Good),
    ("2.16.840.1.101.3.4.3.2", "dsa-with-SHA256", Strength::Good),
    ("1.3.101.112", "Ed25519", Strength::Good),
    ("1.3.101.113", "Ed448", Strength::Good),
];

/// A signature algorithm's name and strength; an unknown one is its dotted form, taken as good.
#[must_use]
pub fn signature(dotted: &str) -> (String, Strength) {
    SIGNATURES
        .iter()
        .find(|(id, _, _)| *id == dotted)
        .map_or_else(
            || (dotted.to_owned(), Strength::Good),
            |(_, name, strength)| ((*name).to_owned(), *strength),
        )
}

/// Named curves: dotted identifier, name, bits.
const CURVES: &[(&str, &str, u32)] = &[
    ("1.2.840.10045.3.1.1", "P-192", 192),
    ("1.3.132.0.33", "P-224", 224),
    ("1.2.840.10045.3.1.7", "P-256", 256),
    ("1.3.132.0.34", "P-384", 384),
    ("1.3.132.0.35", "P-521", 521),
    ("1.3.132.0.10", "secp256k1", 256),
    ("1.3.36.3.3.2.8.1.1.7", "brainpoolP256r1", 256),
    ("1.3.36.3.3.2.8.1.1.11", "brainpoolP384r1", 384),
    ("1.3.36.3.3.2.8.1.1.13", "brainpoolP512r1", 512),
];

/// A named curve's name and size, from its encoded identifier.
#[must_use]
pub fn curve(encoded: &[u8]) -> Option<(&'static str, u32)> {
    let dotted = dotted(encoded)?;
    CURVES
        .iter()
        .find(|(id, _, _)| *id == dotted)
        .map(|(_, name, bits)| (*name, *bits))
}

/// Algorithms named in an encrypted key's parameters: key derivation, ciphers, PKCS#12 schemes.
const CIPHERS: &[(&str, &str)] = &[
    ("1.2.840.113549.1.5.13", "PBES2"),
    ("1.2.840.113549.1.5.12", "PBKDF2"),
    ("1.3.6.1.4.1.11591.4.11", "scrypt"),
    ("2.16.840.1.101.3.4.1.2", "AES-128-CBC"),
    ("2.16.840.1.101.3.4.1.22", "AES-192-CBC"),
    ("2.16.840.1.101.3.4.1.42", "AES-256-CBC"),
    ("1.2.840.113549.3.7", "DES-EDE3-CBC"),
    ("1.3.14.3.2.7", "DES-CBC"),
    ("1.2.840.113549.1.5.3", "pbeWithMD5AndDES-CBC"),
    ("1.2.840.113549.1.5.10", "pbeWithSHA1AndDES-CBC"),
    ("1.2.840.113549.1.12.1.3", "pbeWithSHAAnd3-KeyTripleDES-CBC"),
    ("1.2.840.113549.1.12.1.6", "pbeWithSHAAnd40BitRC2-CBC"),
];

/// A cipher's or a key derivation's name; an unknown one is its dotted form.
#[must_use]
pub fn cipher(encoded: &[u8]) -> String {
    let Some(dotted) = dotted(encoded) else {
        return "?".to_owned();
    };
    CIPHERS
        .iter()
        .find(|(id, _)| *id == dotted)
        .map_or(dotted, |(_, name)| (*name).to_owned())
}

#[cfg(test)]
#[path = "oids_tests.rs"]
mod tests;
