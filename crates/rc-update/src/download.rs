//! The verified download (RC-100-05): an artifact streamed to disk and kept only if it is exactly
//! the file the signed manifest describes. The install of RC-100-06 uses what this returns and
//! nothing else.
//!
//! The bytes go to `<name>.partial` beside the target and are hashed on the way; the size is held
//! to the manifest's at every chunk, so a server that sends more is cut off. Only a file whose
//! size and SHA-256 both match is renamed to its final name; anything else is deleted. A verified
//! file stays until the update that installs it is cleaned up, and is hashed again before it is
//! used ([`verified_file`]).
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/download.rs` at commit 26c8592a6:
//! progress and cancel through [`Progress`], the free-space rule.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{Fetcher, error::UpdateError, manifest::Artifact, sign::hex};

/// The download's progress and its cancel switch, shared with whoever shows or stops it.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    received: Arc<AtomicU64>,
    cancelled: Arc<AtomicBool>,
}

impl Progress {
    /// Bytes written so far.
    #[must_use]
    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    /// Stops the download at its next chunk; it leaves no file.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Refuses a download without twice the artifact's size free (the archive and its unpacked
/// copy); `free` is the volume's, `None` when it does not say.
///
/// # Errors
///
/// `update.no_space`.
pub fn check_space(artifact: &Artifact, free: Option<u64>) -> Result<(), UpdateError> {
    let required = artifact.size.saturating_mul(2);
    match free {
        Some(free) if free < required => Err(UpdateError::NoSpace { free, required }),
        _ => Ok(()),
    }
}

/// Downloads `artifact` into `directory` and returns the verified file's path.
///
/// # Errors
///
/// `update.download_failed`, `update.size_mismatch`, `update.digest_mismatch`,
/// `update.cancelled`, or `update.failed` for the disk; no file stays under the final name then.
pub async fn download_verified(
    fetcher: &dyn Fetcher,
    artifact: &Artifact,
    directory: &Path,
    progress: &Progress,
) -> Result<PathBuf, UpdateError> {
    let url = https_url(artifact)?;
    let target = directory.join(file_name(&url));
    let partial = directory.join(format!("{}.partial", file_name(&url)));
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|error| UpdateError::Other(error.into()))?;
    match stream_to(fetcher, &url, artifact, &partial, progress).await {
        Ok(()) => {
            tokio::fs::rename(&partial, &target)
                .await
                .map_err(|error| UpdateError::Other(error.into()))?;
            Ok(target)
        }
        Err(error) => {
            let _ = tokio::fs::remove_file(&partial).await;
            Err(error)
        }
    }
}

/// The file an earlier [`download_verified`] of `artifact` left in `directory`, while it is still
/// exactly the one the manifest describes: its size and SHA-256 are read again.
pub async fn verified_file(artifact: &Artifact, directory: &Path) -> Option<PathBuf> {
    let target = directory.join(file_name(&https_url(artifact).ok()?));
    let mut file = tokio::fs::File::open(&target).await.ok()?;
    if file.metadata().await.ok()?.len() != artifact.size {
        return None;
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await.ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    (hex(&hasher.finalize()) == artifact.sha256).then_some(target)
}

fn https_url(artifact: &Artifact) -> Result<url::Url, UpdateError> {
    let url = url::Url::parse(&artifact.url)
        .map_err(|error| UpdateError::Invalid(format!("artifact URL: {error}")))?;
    if url.scheme() != "https" {
        return Err(UpdateError::Invalid(format!("{url} is not https")));
    }
    Ok(url)
}

async fn stream_to(
    fetcher: &dyn Fetcher,
    url: &url::Url,
    artifact: &Artifact,
    partial: &Path,
    progress: &Progress,
) -> Result<(), UpdateError> {
    let mut download = fetcher
        .open(url)
        .await
        .map_err(|error| UpdateError::Download(format!("{error:#}")))?;
    if let Some(length) = download.length
        && length != artifact.size
    {
        return Err(UpdateError::SizeMismatch {
            expected: artifact.size,
            saw: length,
        });
    }
    let mut file = tokio::fs::File::create(partial)
        .await
        .map_err(|error| UpdateError::Other(error.into()))?;
    let mut hasher = Sha256::new();
    let mut written: u64 = 0;
    loop {
        if progress.is_cancelled() {
            return Err(UpdateError::Cancelled);
        }
        let Some(chunk) = download
            .body
            .next_chunk()
            .await
            .map_err(|error| UpdateError::Download(format!("{error:#}")))?
        else {
            break;
        };
        written = written.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if written > artifact.size {
            return Err(UpdateError::SizeMismatch {
                expected: artifact.size,
                saw: written,
            });
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|error| UpdateError::Other(error.into()))?;
        progress.received.store(written, Ordering::Relaxed);
    }
    file.sync_all()
        .await
        .map_err(|error| UpdateError::Other(error.into()))?;
    drop(file);
    if written != artifact.size {
        return Err(UpdateError::SizeMismatch {
            expected: artifact.size,
            saw: written,
        });
    }
    if hex(&hasher.finalize()) != artifact.sha256 {
        return Err(UpdateError::DigestMismatch);
    }
    Ok(())
}

/// The URL's last segment, reduced to characters that are safe in a file name everywhere.
pub(crate) fn file_name(url: &url::Url) -> String {
    let last = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or_default();
    let cleaned: String = last
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
        .collect();
    let cleaned = cleaned.trim_start_matches('.');
    if cleaned.is_empty() {
        "rcommander-update.bin".to_owned()
    } else {
        cleaned.to_owned()
    }
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
