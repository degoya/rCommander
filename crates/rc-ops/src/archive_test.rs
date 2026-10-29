//! The archive test (Alt+Shift+F9, RC-030-02): every file of an archive read to its end, so the
//! reader that unpacks it checks what the format stores - the CRC of a zip or 7z entry, the
//! check of a gzip, xz, bzip2 or zstd stream. Nothing is written.
//!
//! Nobody is asked: a damaged entry and an encrypted one (no password is asked for) are recorded
//! as failures with their code (`archive_damaged`, `password_required`, ...) and the test goes
//! on; links and special entries have nothing to check and count as skipped. A listing that
//! fails goes through `on_error` like in every other job.

use std::path::Path;

use rc_vfs::{BackendId, EntryKind, VfsPath};
use tokio::io::AsyncReadExt;

use crate::{
    Failure, OpsError,
    runner::{Runner, Step, Stop, attempt},
};

impl Runner {
    pub(crate) async fn test_archive(&mut self, archive: &VfsPath) -> Result<(), Stop> {
        // The archive file itself passes the read guard of an MCP job first (RC-080-42).
        if let BackendId::Archive { outer, .. } = &archive.backend
            && self.read_guard.is_some()
        {
            let admitted = attempt!(
                self,
                archive,
                self.admit_on(&outer.backend, &outer.path)
                    .await
                    .map_err(Step::from)
            );
            if admitted.is_none() {
                return Ok(());
            }
        }
        let items = self.plan(std::slice::from_ref(archive)).await?;
        let mut buffer = vec![0_u8; self.buffer_size];
        for item in items.iter().filter(|item| item.is_file()) {
            self.checkpoint().await?;
            if item.entry.kind != EntryKind::File {
                self.count_skipped(item.entry.size);
                continue;
            }
            let at = self.src_path(&item.path);
            self.progress.begin_file(at.clone(), item.entry.size);
            let before = self.progress.bytes_done;
            match self.read_through(&item.path, &mut buffer).await {
                Ok(()) => {}
                Err(Step::Stop(stop)) => return Err(stop),
                Err(Step::Fail(error)) => {
                    let failure = Failure::new(&at, &error);
                    self.job.update(|snapshot| snapshot.failures.push(failure));
                }
            }
            let read = self.progress.bytes_done.saturating_sub(before);
            self.count_done(1, item.entry.size.saturating_sub(read));
        }
        Ok(())
    }

    /// Reads the file `path` of the source to its end, a buffer at a time, counting the bytes.
    async fn read_through(&mut self, path: &Path, buffer: &mut [u8]) -> Result<(), Step> {
        let mut reader = self.src.open_read(path).await?;
        loop {
            self.checkpoint().await?;
            let read = self.job.control.or_cancel(reader.read(buffer)).await?;
            let count = read.map_err(|error| OpsError::from_io(error, path))?;
            if count == 0 {
                return Ok(());
            }
            self.progress.count(count as u64);
            self.tick(false);
        }
    }
}
