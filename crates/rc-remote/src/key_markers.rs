//! The one table of text that marks key material (RC-080-43): what the MCP mask hides
//! (`rc-engine`'s `viewer::secret`), what stops an SSH configuration import (`import::ssh_config`)
//! and what the key reader starts from ([`crate::private_key`]) all come from here.

/// The end of a PEM private key's armour line, opening or closing: OpenSSH's, PKCS#1's
/// `RSA PRIVATE KEY`, PKCS#8's plain and encrypted `PRIVATE KEY`, each closed by five dashes.
pub const PEM_PRIVATE: &str = "PRIVATE KEY-----";

/// How PuTTY starts a key file, followed by the format version (`PuTTY-User-Key-File-3:`).
pub const PPK_HEADER: &str = "PuTTY-User-Key-File-";

/// The line of a PuTTY key that opens its private half.
pub const PPK_PRIVATE_LINES: &str = "Private-Lines:";

/// The last line of a PuTTY key.
pub const PPK_MAC: &str = "Private-MAC:";

/// Text that only private key material carries.
pub const PRIVATE_KEY_MARKERS: [&str; 4] = [PEM_PRIVATE, PPK_HEADER, PPK_PRIVATE_LINES, PPK_MAC];

/// PEM armour: opens a private key as much as a public key or a certificate.
pub const PEM_ARMOUR: &str = "-----BEGIN";

/// Whether `text` carries a private key marker.
#[must_use]
pub fn holds_private_key(text: &str) -> bool {
    PRIVATE_KEY_MARKERS
        .iter()
        .any(|marker| text.contains(marker))
}

/// Whether `text` holds key material of any kind: a private key marker, or PEM armour.
#[must_use]
pub fn holds_key_material(text: &str) -> bool {
    holds_private_key(text) || text.contains(PEM_ARMOUR)
}

#[cfg(test)]
#[path = "key_markers_tests.rs"]
mod tests;
