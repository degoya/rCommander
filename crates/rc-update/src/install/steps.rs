//! The steps around the switch (RC-100-06): the artifact checked once more and unpacked from the
//! handle that was hashed (T-SWAP), the database copy put back after a failed start, and the
//! leftovers of a finished update.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/install/steps.rs` at commit
//! 26c8592a6: no installer kept; the database copy is put back only while its BLAKE3 matches.

use std::fs;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

use super::{
    InstallError, Journal, Phase, Plan,
    durable::{exists, remove_any, rename, sync_directory},
    process::UPDATER_DIR,
    trash, update_dir,
};
use crate::sign::hex;

/// 64 lowercase hexadecimal digits: a SHA-256 or a BLAKE3.
#[must_use]
pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Hashes the downloaded artifact again: it lay in the data directory between the check and
/// this use, and only what the signed manifest describes is ever installed.
///
/// # Errors
///
/// `update.digest_mismatch` when size or SHA-256 differ, `update.download_failed` when it is gone.
pub fn verify_artifact(plan: &Plan) -> Result<(), InstallError> {
    open_artifact(plan).map(drop)
}

/// The plan's artifact, opened and checked: the handle the switch unpacks from.
///
/// # Errors
///
/// As [`verify_artifact`].
pub fn open_artifact(plan: &Plan) -> Result<fs::File, InstallError> {
    open_checked(&plan.artifact, &plan.sha256, plan.size)
}

/// Opens `path`, checks its size and SHA-256 and returns the handle at the file's start: what is
/// read through it is what was hashed, where hashing one opening and using the path again would
/// let a file swapped in between be installed. On Windows the handle shares reading only, so
/// while it is open nobody writes, renames or removes the file.
///
/// # Errors
///
/// `update.digest_mismatch` or `update.download_failed`.
pub(crate) fn open_checked(path: &Path, sha256: &str, size: u64) -> Result<fs::File, InstallError> {
    let gone = |error: std::io::Error| {
        InstallError::new(
            "update.download_failed",
            format!("{}: {error}", path.display()),
        )
    };
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        options.share_mode(FILE_SHARE_READ);
    }
    let mut file = options.open(path).map_err(gone)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 16];
    let mut read_bytes: u64 = 0;
    loop {
        let read = file.read(&mut buffer).map_err(gone)?;
        if read == 0 {
            break;
        }
        read_bytes = read_bytes.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    if size != read_bytes || hex(&hasher.finalize()) != sha256 {
        return Err(InstallError::new(
            "update.digest_mismatch",
            format!("{} is not the file that was checked", path.display()),
        ));
    }
    file.seek(SeekFrom::Start(0)).map_err(gone)?;
    Ok(file)
}

/// The BLAKE3 of a file, hex.
///
/// # Errors
///
/// When the file cannot be read.
pub fn blake3_of(path: &Path) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    std::io::copy(&mut file, &mut hasher).with_context(|| format!("read {}", path.display()))?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Puts the database copy written before the update back in place: the new version was started
/// and may have migrated the database to a schema the old one cannot open. The copy is put back
/// only while its BLAKE3 is the journal's; the live files (with `-wal` and `-shm`) go to
/// `<data>/update/replaced-database/`. Returns whether a copy was put back; run again after a
/// stop, it puts it back again, which is the same result.
///
/// # Errors
///
/// When the copy is gone or changed, or a step fails.
pub fn restore_database(plan: &Plan) -> Result<bool> {
    let Some(copy) = plan.database_copy.as_ref() else {
        return Ok(false);
    };
    ensure!(
        copy.is_file(),
        "the database copy {} is gone",
        copy.display()
    );
    let expected = plan
        .database_copy_blake3
        .as_deref()
        .context("the journal names no digest of the database copy")?;
    ensure!(
        blake3_of(copy)? == expected,
        "the database copy {} is not the one written before the update",
        copy.display()
    );
    let name = plan
        .database
        .file_name()
        .context("the database has no file name")?
        .to_string_lossy()
        .into_owned();
    let aside = update_dir(&plan.data_dir).join("replaced-database");
    remove_any(&aside)?;
    fs::create_dir_all(&aside).with_context(|| format!("create {}", aside.display()))?;
    for suffix in ["", "-wal", "-shm"] {
        let live = plan.database.with_file_name(format!("{name}{suffix}"));
        if exists(&live) {
            rename(&live, &aside.join(format!("{name}{suffix}")))?;
        }
    }
    let restoring = plan.database.with_file_name(format!("{name}.restoring"));
    fs::copy(copy, &restoring)
        .with_context(|| format!("copy {} to {}", copy.display(), restoring.display()))?;
    fs::OpenOptions::new()
        .write(true)
        .open(&restoring)
        .and_then(|file| file.sync_all())
        .with_context(|| format!("sync {}", restoring.display()))?;
    rename(&restoring, &plan.database)?;
    if let Some(parent) = plan.database.parent() {
        sync_directory(parent);
    }
    Ok(true)
}

/// Removes what a finished update leaves beside the program - the staging, the files of a
/// rolled-back version and, after a proven update, `.previous/` - through the trash, and the
/// replaced database and the updater's copy in the data folder. The downloads and the database
/// copies stay for the retention (RC-100-07, `retention.rs`).
pub fn clean_up(journal: &Journal) {
    let install = &journal.plan.install_dir;
    let mut beside_program = vec![journal.staged_dir(), journal.failed_dir()];
    if journal.phase == Phase::Verified {
        beside_program.push(journal.previous_dir());
    }
    for leftover in beside_program {
        if let Err(error) = trash::discard(&leftover, install) {
            tracing::warn!(
                error = format!("{error:#}"),
                "an update leftover stays until a later start"
            );
        }
    }
    let update = update_dir(&journal.plan.data_dir);
    for leftover in [update.join("replaced-database"), update.join(UPDATER_DIR)] {
        if let Err(error) = remove_any(&leftover) {
            tracing::warn!(%error, "an update leftover stays until a later start");
        }
    }
}
