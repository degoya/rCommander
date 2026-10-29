//! What crosses a crate boundary holding a secret: the master password and decrypted values,
//! all zeroized on drop and printed by `Debug` as `[REDACTED]`; none has `Display`.

use std::fmt;

pub use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroize as _;

/// Decrypted bytes: zeroized on drop, never printed.
pub type SecretBytes = secrecy::SecretSlice<u8>;

/// The master password as typed. Zeroized on drop; `Debug` shows no content.
pub struct MasterPassword(SecretString);

impl MasterPassword {
    /// Wraps a typed password. The `String` is zeroized here; the copy kept has exactly its
    /// length, so boxing it leaves no unzeroized spare buffer behind.
    #[must_use]
    pub fn new(mut password: String) -> Self {
        let kept = SecretString::from(password.as_str().to_owned());
        password.zeroize();
        Self(kept)
    }

    /// A second, independent copy, for work on another thread.
    pub(crate) fn duplicate(&self) -> Self {
        Self(SecretString::from(self.0.expose_secret().to_owned()))
    }

    /// Whether nothing was typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.expose_secret().is_empty()
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.0.expose_secret().as_bytes()
    }
}

impl From<String> for MasterPassword {
    fn from(password: String) -> Self {
        Self::new(password)
    }
}

impl From<SecretString> for MasterPassword {
    fn from(password: SecretString) -> Self {
        Self(password)
    }
}

impl fmt::Debug for MasterPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MasterPassword([REDACTED])")
    }
}

/// Where an encrypted value lives: its table, column and row id. The three form the value's
/// associated data, `"<table>/<column>/<row id>"`, so a value only decrypts where it was
/// written. Table and column are identifiers (no `/`), which keeps the form unambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldRef<'a> {
    pub table: &'a str,
    pub column: &'a str,
    pub row_id: &'a str,
}

impl<'a> FieldRef<'a> {
    #[must_use]
    pub fn new(table: &'a str, column: &'a str, row_id: &'a str) -> Self {
        Self {
            table,
            column,
            row_id,
        }
    }

    /// The associated data.
    pub(crate) fn associated_data(&self) -> Vec<u8> {
        format!("{}/{}/{}", self.table, self.column, self.row_id).into_bytes()
    }
}
