//! The portable switch (RC-100-06): the program files of an unpacked archive, replaced by the
//! entries of the new one.
//!
//! 1. **Stage** ([`stage`]): the verified zip or `.tar.gz` is unpacked from the handle that was
//!    hashed (T-SWAP) into `<install>/.update-<version>/`, the archive's version folder
//!    (`rcommander-<v>-windows-x64/`) stripped; its top-level names are the entries the switch
//!    moves, those that exist now the ones it replaces. The journal says `staged`.
//! 2. **Switch** ([`switch`]): per entry, the live one into `<install>/.previous/`, the new one
//!    into its place. Every step looks at what is where, so a switch stopped anywhere is finished
//!    by running it again and taken back by [`roll_back`] from any point.
//! 3. **Roll back** ([`roll_back`]): a new entry that left the staging goes to
//!    `<install>/.failed-<version>/`, each entry in `.previous/` back to its place.
//!
//! Names the switch never moves, even when an archive carries them: [`KEPT`] and anything hidden.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/install/portable.rs` at commit
//! 26c8592a6: the version folder stripped, the program a relative path, no crash points.

use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};

use super::{
    InstallError, Journal, Phase,
    durable::{exists, remove_any, rename, sync_directory},
    steps, trash,
};

/// What may live beside a portable program and is never replaced by an update: a data folder
/// kept there, and the marker a later installer writes (`install_kind.rs`).
pub const KEPT: &[&str] = &["data", "install-kind"];

/// Unpacks the verified artifact beside the program, records the entries it brings and which of
/// them exist now, and writes the journal as `staged`.
///
/// # Errors
///
/// [`InstallError`] `update.digest_mismatch` for a file that is not the one checked,
/// `update.archive_unknown` for no archive this knows, `update.archive_incomplete` for one without
/// the program; otherwise what failed. Nothing live has changed then.
pub fn stage(journal: &mut Journal) -> Result<()> {
    let staged = journal.staged_dir();
    let install = journal.plan.install_dir.clone();
    let unpacking = install.join(format!(".update-{}.unpack", journal.plan.target_version));
    // Leftovers of an earlier update go through the trash, never removed in place: a program
    // started before that update may still run from `.previous/`.
    trash::sweep(&install);
    for leftover in [
        staged.clone(),
        unpacking.clone(),
        journal.previous_dir(),
        journal.failed_dir(),
    ] {
        trash::discard(&leftover, &install)?;
    }
    let archive = steps::open_artifact(&journal.plan)?;
    fs::create_dir_all(&unpacking).with_context(|| format!("create {}", unpacking.display()))?;
    unpack(&journal.plan.artifact, archive, &unpacking)?;
    let root = archive_root(&unpacking, journal.plan.program_entry())?;
    rename(&root, &staged)?;
    remove_any(&unpacking)?;
    let entries = entries_of(&staged, &journal.plan.executable)?;
    journal.replaced = entries
        .iter()
        .filter(|name| exists(&install.join(name)))
        .cloned()
        .collect();
    journal.entries = entries;
    journal.advance(Phase::Staged)
}

/// Unpacks `file`, the opened `artifact`, whose name says what it is. Both readers keep every
/// entry inside `into` (zip's enclosed names, tar's refusal of `..`).
fn unpack(artifact: &Path, file: fs::File, into: &Path) -> Result<()> {
    let name = artifact
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(BufReader::new(file))
            .with_context(|| format!("read {}", artifact.display()))?;
        archive
            .extract(into)
            .with_context(|| format!("unpack {}", artifact.display()))?;
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(BufReader::new(file)));
        archive.set_overwrite(true);
        archive
            .unpack(into)
            .with_context(|| format!("unpack {}", artifact.display()))?;
    } else {
        bail!(InstallError::new(
            "update.archive_unknown",
            format!("{} is neither a .zip nor a .tar.gz", artifact.display()),
        ));
    }
    Ok(())
}

/// The folder the archive's entries are in: its one top-level folder when it has nothing else
/// (the package's `rcommander-<v>-<platform>-<arch>/`), unless that folder is the program's own
/// entry (a macOS `.app` bundle); `unpacked` itself otherwise.
fn archive_root(unpacked: &Path, program_entry: &str) -> Result<PathBuf> {
    let mut entries = fs::read_dir(unpacked)
        .with_context(|| format!("list {}", unpacked.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("list {}", unpacked.display()))?;
    if entries.len() == 1 {
        let only = entries.remove(0);
        let name = only.file_name();
        if only.file_type().is_ok_and(|kind| kind.is_dir()) && name.to_str() != Some(program_entry)
        {
            return Ok(only.path());
        }
    }
    Ok(unpacked.to_owned())
}

/// The top-level names of the staged archive, sorted, without what the switch never moves; the
/// program must be among them.
fn entries_of(staged: &Path, executable: &str) -> Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(staged).with_context(|| format!("list {}", staged.display()))? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|name| anyhow!("the archive entry {name:?} is not UTF-8"))?;
        if name.starts_with('.') || KEPT.iter().any(|kept| kept.eq_ignore_ascii_case(&name)) {
            tracing::warn!(entry = %name, "the update archive carries an entry the switch never replaces; it is left out");
            continue;
        }
        entries.push(name);
    }
    entries.sort();
    let program = executable
        .split('/')
        .fold(staged.to_owned(), |path, part| path.join(part));
    ensure!(
        program.is_file(),
        InstallError::new(
            "update.archive_incomplete",
            format!("the archive carries no {executable}"),
        )
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = fs::metadata(&program)?.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        fs::set_permissions(&program, permissions)?;
    }
    Ok(entries)
}

/// Sets every entry that exists aside into `.previous/` and puts the new one in its place, then
/// writes the journal as `switched`. Idempotent: run again after a stop, it finishes.
///
/// # Errors
///
/// When a rename fails or the files are in a state no switch leaves them in; [`roll_back`]
/// takes back whatever was done.
pub fn switch(journal: &mut Journal) -> Result<()> {
    ensure!(
        matches!(journal.phase, Phase::Staged | Phase::Switching),
        "the update is {}, not staged",
        journal.phase.as_str()
    );
    if journal.phase == Phase::Staged {
        journal.advance(Phase::Switching)?;
    }
    let install = journal.plan.install_dir.clone();
    let staged = journal.staged_dir();
    let previous = journal.previous_dir();
    fs::create_dir_all(&previous).with_context(|| format!("create {}", previous.display()))?;
    for name in journal.entries.clone() {
        let live = install.join(&name);
        let aside = previous.join(&name);
        let incoming = staged.join(&name);
        // Once an entry left the staging it is the live one, and a second run leaves it alone.
        if !exists(&incoming) {
            continue;
        }
        if exists(&live) {
            ensure!(
                !exists(&aside),
                "{} and its previous copy both exist; the switch cannot tell them apart",
                live.display()
            );
            // The running program's own file moves too: Windows lets a running `.exe` be
            // renamed, never replaced.
            rename(&live, &aside)?;
        }
        rename(&incoming, &live)?;
    }
    // What stays in the staging is what the switch never moves; it goes with a later clean-up.
    if let Err(error) = trash::discard(&staged, &install) {
        tracing::warn!(%error, "the staging of the update stays until a later start");
    }
    sync_directory(&previous);
    sync_directory(&install);
    journal.advance(Phase::Switched)
}

/// Takes a switch back from any point: every new entry that left the staging goes to
/// `.failed-<version>/`, every entry in `.previous/` back to its place; then the staging and
/// `.previous/` go, `.failed-...` through the trash. The journal is not written; the caller ends
/// it with the reason.
///
/// # Errors
///
/// When a rename fails: the installation then needs the manual recovery of
/// `docs/development.md`, and the journal must say so.
pub fn roll_back(journal: &Journal) -> Result<()> {
    let install = &journal.plan.install_dir;
    let staged = journal.staged_dir();
    let previous = journal.previous_dir();
    let failed = journal.failed_dir();
    for name in &journal.entries {
        let live = install.join(name);
        let aside = previous.join(name);
        let came_out = !exists(&staged.join(name));
        // The live entry is the new one when the old one waits aside, or when the old version
        // did not have it and it has left the staging.
        let live_is_new =
            exists(&live) && (exists(&aside) || (came_out && !journal.replaced.contains(name)));
        if live_is_new {
            fs::create_dir_all(&failed).with_context(|| format!("create {}", failed.display()))?;
            let target = failed.join(name);
            remove_any(&target)?;
            rename(&live, &target)?;
        }
        if exists(&aside) {
            rename(&aside, &live)?;
        }
    }
    sync_directory(install);
    trash::discard(&staged, install)?;
    trash::discard(&previous, install)?;
    if let Err(error) = trash::discard(&failed, install) {
        tracing::warn!(%error, "the files of the rolled-back version stay until a later start");
    }
    Ok(())
}

#[cfg(test)]
#[path = "portable_tests.rs"]
mod tests;
