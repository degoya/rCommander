//! "Verify after copy": the target read back and its BLAKE3 checksum held against the
//! source's, which the copy computed on the way. A move checks at least the size before it
//! removes its source (RC-060-05): the target's against the bytes written, and where those are
//! not the size the job saw, against the source as it is now - a stream that ended early without
//! an error never costs the source (RC-080-37).

use std::path::Path;

use tokio::io::AsyncReadExt;

use crate::{
    OpsError,
    runner::{Runner, Step},
    walk::Item,
};

impl Runner {
    /// The checks a complete copy of `item` at `dest` of `bytes` bytes passes before it counts:
    /// the checksum when the job verifies; when it moves, the size on the target, and the
    /// source's size where `bytes` is not the one the job saw.
    pub(crate) async fn check_copy(
        &mut self,
        item: &Item,
        dest: &Path,
        expected: &blake3::Hash,
        bytes: u64,
    ) -> Result<(), Step> {
        if self.options.verify {
            self.verify(dest, expected).await?;
        }
        if !self.moving {
            return Ok(());
        }
        if self.dst.stat(dest).await?.size != bytes {
            return Err(OpsError::VerifyFailed {
                path: dest.to_path_buf(),
            }
            .into());
        }
        // A source that changed since it was listed may hold what was read; any other is short.
        if bytes != item.entry.size && self.src.stat(&item.path).await?.size != bytes {
            return Err(OpsError::VerifyFailed {
                path: item.path.clone(),
            }
            .into());
        }
        Ok(())
    }

    /// Reads `dest` back; `verify_failed` when its checksum is not `expected`.
    pub(crate) async fn verify(
        &mut self,
        dest: &Path,
        expected: &blake3::Hash,
    ) -> Result<(), Step> {
        let mut reader = self.dst.open_read(dest).await?;
        let mut buffer = vec![0_u8; self.buffer_size];
        let mut hasher = blake3::Hasher::new();
        loop {
            self.checkpoint().await?;
            let read = self.job.control.or_cancel(reader.read(&mut buffer)).await?;
            let count = read.map_err(|error| OpsError::from_io(error, dest))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        if hasher.finalize() == *expected {
            Ok(())
        } else {
            Err(OpsError::VerifyFailed {
                path: dest.to_path_buf(),
            }
            .into())
        }
    }
}
