//! `Engine::dispatch` for the `checksum` group (`registry/checksum.rs`).

/// The `checksum` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ComputeChecksums(crate::ChecksumRequest {
                panel,
                paths,
                algorithm,
            }) => $self.compute_checksums(panel, &paths, algorithm).await,
            Command::CreateChecksumFiles(crate::ChecksumFilesRequest {
                panel,
                dir,
                names,
                algorithm,
                per_file,
                file_name,
            }) => {
                let layout = if per_file {
                    rc_checksum::Layout::PerFile
                } else {
                    rc_checksum::Layout::Combined { file_name }
                };
                $self.create_checksum_files(panel, dir.as_deref(), &names, algorithm, &layout)
                    .await
            }
            Command::ChecksumDialog { panel, dialog } => Ok($self.checksum_dialog(panel, dialog)),
            Command::QueueChecksumJob(job) => $self.queue_checksum_job(job).await,
            Command::VerifyChecksumFile { panel, path } => {
                $self.verify_checksum_file(panel, &path).await
            }
        } }
    };
}
pub(crate) use arms;
