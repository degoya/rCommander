//! The password of an encrypted zip or 7z (RC-030-03): what a person typed for one job.
//!
//! It is held as a [`SecretString`] - zeroized when dropped, `[redacted]` in `Debug` - and handed
//! to the archive crates only inside the blocking task that reads. It is never logged, never part
//! of an error and never stored; the job that asked for it drops it when it ends.
//!
//! A wrong password is told from a missing one: the zip check bytes and a 7z header that will
//! not decode say `password_wrong`, and so does damaged data in an encrypted entry once a password
//! was given - what 7-Zip reports as "wrong password?".

use std::{fmt, path::Path};

use secrecy::{ExposeSecret, SecretString};

use crate::VfsError;

/// A password for the encrypted entries of one archive.
#[derive(Clone)]
pub struct ArchivePassword(SecretString);

impl ArchivePassword {
    /// Wraps a typed password.
    #[must_use]
    pub fn new(password: SecretString) -> Self {
        Self(password)
    }

    /// The bytes zip encryption works on.
    pub(crate) fn bytes(&self) -> &[u8] {
        self.0.expose_secret().as_bytes()
    }

    /// The password as the 7z crate takes it (UTF-16, as 7-Zip derives the key).
    pub(crate) fn sevenz(&self) -> sevenz_rust2::Password {
        sevenz_rust2::Password::from(self.0.expose_secret())
    }
}

impl fmt::Debug for ArchivePassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArchivePassword([redacted])")
    }
}

/// An error reading an encrypted entry with `password` given: damage there means the password
/// did not fit; every other error, and every error without a password, stays as it is.
pub(crate) fn judge(error: VfsError, password: Option<&ArchivePassword>, path: &Path) -> VfsError {
    match error {
        VfsError::ArchiveDamaged { .. } | VfsError::PasswordRequired { .. }
            if password.is_some() =>
        {
            VfsError::PasswordWrong {
                path: path.to_path_buf(),
            }
        }
        other => other,
    }
}

#[cfg(test)]
#[path = "password_tests.rs"]
mod tests;
