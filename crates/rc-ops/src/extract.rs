//! Unpacking (RC-030-03): archives into a directory, as a job of the queue.
//!
//! Each archive is opened as an [`ArchiveFs`] over the backend it lies on, its tree is walked and
//! every file is copied out in the archive's own order - one pass through a solid 7z block or a
//! compressed tar, not one per file (RC-080-46) - with the copy's own steps (`copy.rs`): conflicts
//! asked as for any copy,
//! the file written under a part name and renamed once complete, so a damaged entry or a cancel
//! leaves the file it would replace as it was (RC-080-37), times and permissions kept. On top of
//! the archive backend's guards - cleaned names, links never followed, the limits against archive
//! bombs - unpacking writes nothing it cannot place safely:
//!
//! - an entry whose stored name was unsafe (zip slip `..`, absolute, drive, UNC) is skipped and
//!   listed (`unsafe_entry`), never written under its cleaned name;
//! - every target path is built from checked names (`target_path.rs`) and stays below the target;
//!   a name Windows cannot hold (`a:b`, `con`, a trailing dot) is skipped the same way;
//! - links stored in the archive are not created (`link_not_extracted`);
//! - a folder on the way that is a link or junction in the target is refused, so a link already
//!   there cannot lead the files elsewhere, and an existing target file that is a link is not
//!   written through;
//! - set-user-id and the like are dropped: only the permission bits are kept.
//!
//! An encrypted archive asks for its password ([`crate::Question::Password`]); a wrong one asks
//! again, a skipped one fails its encrypted entries with `password_required`. A job that may not
//! ask (an `on_error` preset, every MCP job) fails them at once. The password lives in this job
//! only and is dropped (zeroized) with it.

mod target_path;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::{
    ArchiveFormat, ArchiveFs, ArchivePassword, BackendId, EntryKind, Vfs, VfsError, VfsPath,
    detect_format_within,
};
use serde::{Deserialize, Serialize};

use crate::{
    ErrorChoice, Failure, JobState, OpsError,
    mask::Mask,
    runner::{Runner, Step, Stop, attempt},
    staging::Resolved,
    walk::Item,
};

/// How an extraction runs; the defaults are Total Commander's unpack dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtractSettings {
    /// Each archive into a folder of its own below the target, named after it (`photos.zip`
    /// into `photos`).
    pub own_folder: bool,
    /// The files to unpack, by name: `*.*` (every file), `*.txt;*.md`, `*|*.tmp`.
    pub mask: String,
    /// Keep the archive's folders; off puts every file straight into the target.
    pub keep_paths: bool,
}

impl Default for ExtractSettings {
    fn default() -> Self {
        Self {
            own_folder: false,
            mask: "*.*".to_owned(),
            keep_paths: true,
        }
    }
}

/// One archive, opened and walked.
struct Plan {
    /// The backend the archive file lies on.
    outer: Arc<dyn Vfs>,
    archive: VfsPath,
    format: ArchiveFormat,
    fs: Arc<ArchiveFs>,
    password: Option<ArchivePassword>,
    /// The person skipped the password: encrypted entries fail without asking again.
    declined: bool,
    entries: Vec<Planned>,
}

/// One entry of a plan.
struct Planned {
    /// `path` inside the archive, `rel` below the target, the entry.
    item: Item,
    /// Why it is not written; listed as a failure when its turn comes.
    refused: Option<OpsError>,
    /// Where it lies in the archive; `None` for a folder the archive does not state.
    position: Option<u64>,
}

/// Whether `error` asks for a (better) password.
fn needs_password(error: &OpsError) -> bool {
    matches!(
        error,
        OpsError::Vfs(VfsError::PasswordRequired { .. } | VfsError::PasswordWrong { .. })
    )
}

/// The folder an archive unpacks into with `own_folder`: its name without the archive endings
/// (`site.tar.gz` gives `site`).
pub(crate) fn folder_name(file: &str) -> String {
    const ENDINGS: &[&str] = &[
        ".gz", ".xz", ".bz2", ".zst", ".tgz", ".txz", ".tbz2", ".tzst", ".tar", ".zip", ".7z",
    ];
    let mut name = file;
    while let Some(stem) = ENDINGS.iter().find_map(|ending| {
        let cut = name.len().checked_sub(ending.len())?;
        let (stem, tail) = (name.get(..cut)?, name.get(cut..)?);
        tail.eq_ignore_ascii_case(ending).then_some(stem)
    }) {
        if stem.is_empty() {
            break;
        }
        name = stem;
    }
    name.to_owned()
}

/// Makes the folders of `rel` below `root`, refusing any that is not a real folder - a link or a
/// junction there could lead the files outside the target.
async fn make_dirs(
    dst: &dyn Vfs,
    root: &Path,
    rel: &Path,
    verified: &mut HashSet<PathBuf>,
) -> Result<(), Step> {
    let mut at = root.to_path_buf();
    for part in rel.components() {
        at.push(part);
        if verified.contains(&at) {
            continue;
        }
        match dst.stat(&at).await {
            Ok(entry) if entry.kind == EntryKind::Dir => {}
            Ok(entry) if matches!(entry.kind, EntryKind::Symlink { .. }) => {
                return Err(OpsError::UnsafeEntry {
                    path: at,
                    reason: "a link in the target folder",
                }
                .into());
            }
            Ok(_) => return Err(OpsError::KindMismatch { path: at }.into()),
            Err(VfsError::NotFound { .. }) => dst.mkdir(&at, false).await?,
            Err(error) => return Err(error.into()),
        }
        verified.insert(at.clone());
    }
    Ok(())
}

impl Runner {
    /// Unpacks every archive into the directory `target`.
    pub(crate) async fn extract(
        &mut self,
        archives: &[VfsPath],
        target: &VfsPath,
        settings: &ExtractSettings,
    ) -> Result<(), Stop> {
        self.rename_ok = false;
        self.staged = crate::staging::is_staged(&target.backend);
        let outer = Arc::clone(&self.src);
        let dst = Arc::clone(&self.dst);
        if attempt!(
            self,
            target,
            crate::copy::ensure_dir(dst.as_ref(), &target.path).await
        )
        .is_none()
        {
            return Ok(());
        }
        let mask = Mask::parse_or_all(&settings.mask);
        let mut plans = Vec::with_capacity(archives.len());
        for archive in archives {
            self.checkpoint().await?;
            let opened = attempt!(self, archive, {
                // The archive file passes the read guard of an MCP job first (RC-080-42).
                match self.admit(&archive.path).await {
                    Err(refused) => Err(refused.into()),
                    Ok(()) => {
                        self.open_archive(&outer, archive, target, settings, &mask)
                            .await
                    }
                }
            });
            plans.extend(opened);
        }
        self.job
            .update(|snapshot| snapshot.state = JobState::Running);
        self.tick(true);
        let mut verified = HashSet::new();
        for mut plan in plans {
            self.src_id = BackendId::Archive {
                outer: Box::new(plan.archive.clone()),
                format: plan.format,
            };
            self.src = Arc::clone(&plan.fs) as Arc<dyn Vfs>;
            let entries = std::mem::take(&mut plan.entries);
            for entry in &entries {
                self.checkpoint().await?;
                self.extract_entry(&mut plan, entry, target, &mut verified)
                    .await?;
            }
        }
        Ok(())
    }

    /// Detects, opens and walks one archive, asking for its password when its listing needs one
    /// (a 7z with an encrypted header). The totals count only once the walk is complete.
    async fn open_archive(
        &mut self,
        outer: &Arc<dyn Vfs>,
        archive: &VfsPath,
        target: &VfsPath,
        settings: &ExtractSettings,
        mask: &Mask,
    ) -> Result<Plan, Step> {
        let format = detect_format_within(outer, &archive.path, self.archive.limits)
            .await?
            .ok_or_else(|| OpsError::NotAnArchive {
                path: archive.path.clone(),
            })?;
        let base = match archive.path.file_name() {
            Some(name) if settings.own_folder => {
                PathBuf::from(folder_name(&name.to_string_lossy()))
            }
            _ => PathBuf::new(),
        };
        let mut password: Option<ArchivePassword> = None;
        loop {
            let fs = self.archive_fs(outer, archive, format, password.as_ref());
            match self.walk_archive(&fs, &base, target, settings, mask).await {
                Ok(entries) => {
                    for planned in entries.iter().filter(|p| p.item.is_file()) {
                        self.progress.files_total += 1;
                        self.progress.bytes_total += planned.item.entry.size;
                    }
                    return Ok(Plan {
                        outer: Arc::clone(outer),
                        archive: archive.clone(),
                        format,
                        fs,
                        password,
                        declined: false,
                        entries,
                    });
                }
                Err(Step::Fail(error)) if needs_password(&error) => {
                    match self.ask_password(archive, password.is_some()).await? {
                        Some(typed) => password = Some(typed),
                        None => return Err(Step::Fail(error)),
                    }
                }
                Err(other) => return Err(other),
            }
        }
    }

    fn archive_fs(
        &self,
        outer: &Arc<dyn Vfs>,
        archive: &VfsPath,
        format: ArchiveFormat,
        password: Option<&ArchivePassword>,
    ) -> Arc<ArchiveFs> {
        let fs = ArchiveFs::new(
            Arc::clone(outer),
            archive.path.clone(),
            format,
            self.archive.clone(),
        );
        Arc::new(match password {
            Some(password) => fs.with_password(password.clone()),
            None => fs,
        })
    }

    /// Every entry of the archive the settings pick, with where it goes or why it does not, in the
    /// archive's order: read so, its producer walks the archive once.
    async fn walk_archive(
        &mut self,
        fs: &ArchiveFs,
        base: &Path,
        target: &VfsPath,
        settings: &ExtractSettings,
        mask: &Mask,
    ) -> Result<Vec<Planned>, Step> {
        let mut planned = Vec::new();
        let mut stack = vec![PathBuf::from("/")];
        while let Some(dir) = stack.pop() {
            self.checkpoint().await?;
            let mut items = fs.list_ordered(&dir).await?;
            items.sort_by(|(a, _), (b, _)| a.entry.name.cmp(&b.entry.name));
            for (item, position) in items {
                let path = dir.join(&item.entry.name);
                let inner = path.strip_prefix("/").unwrap_or(&path);
                let rel = if settings.keep_paths {
                    base.join(inner)
                } else {
                    base.join(&item.entry.name)
                };
                let stored = || PathBuf::from(&item.info.stored_name);
                let mut refused = item.info.unsafe_name.map(|_| OpsError::UnsafeEntry {
                    path: stored(),
                    reason: "a name that leaves the archive",
                });
                match &item.entry.kind {
                    EntryKind::Dir => {
                        stack.push(path.clone());
                        // Stated folders are made even when empty; the others come with files.
                        if !settings.keep_paths || item.info.implied || refused.is_some() {
                            continue;
                        }
                    }
                    _ if !mask.matches(&item.entry.name, false) => continue,
                    EntryKind::File => {}
                    EntryKind::Symlink { .. } => {
                        if refused.is_none() {
                            refused = Some(OpsError::LinkNotExtracted { path: stored() });
                        }
                    }
                    EntryKind::Other => {
                        if refused.is_none() {
                            refused = Some(OpsError::NotCopyable { path: stored() });
                        }
                    }
                }
                if refused.is_none()
                    && let Err(reason) = target_path::parts(&rel)
                        .ok_or("a name the target cannot hold")
                        .and_then(|parts| target_path::below(&target.path, &parts))
                {
                    refused = Some(OpsError::UnsafeEntry {
                        path: stored(),
                        reason,
                    });
                }
                let mut entry = item.entry;
                // Permission bits only: no set-user-id, set-group-id or sticky bit from an archive.
                entry.attributes.mode = entry.attributes.mode.map(|mode| mode & 0o777);
                planned.push(Planned {
                    item: Item { path, rel, entry },
                    refused,
                    position,
                });
            }
        }
        // Stable: entries without a position keep the walk's order, at the end.
        planned.sort_by_key(|entry| entry.position.unwrap_or(u64::MAX));
        Ok(planned)
    }

    /// One entry of `plan`: listed when refused, made when a folder, unpacked when a file.
    async fn extract_entry(
        &mut self,
        plan: &mut Plan,
        planned: &Planned,
        target: &VfsPath,
        verified: &mut HashSet<PathBuf>,
    ) -> Result<(), Stop> {
        let item = &planned.item;
        let size = if item.is_file() { item.entry.size } else { 0 };
        let files = u64::from(item.is_file());
        if let Some(error) = &planned.refused {
            let failure = Failure::new(&self.src_path(&item.path), error);
            self.job.update(|snapshot| snapshot.failures.push(failure));
            self.count_done(files, size);
            return Ok(());
        }
        let dest = target.path.join(&item.rel);
        let at = VfsPath {
            backend: target.backend.clone(),
            path: dest.clone(),
        };
        let dst = Arc::clone(&self.dst);
        if !item.is_file() {
            let _made = attempt!(
                self,
                &at,
                make_dirs(dst.as_ref(), &target.path, &item.rel, verified).await
            );
            return Ok(());
        }
        let parent = item.rel.parent().unwrap_or(Path::new(""));
        let ready = attempt!(self, &at, {
            match make_dirs(dst.as_ref(), &target.path, parent, verified).await {
                Ok(()) => not_a_link(dst.as_ref(), &dest).await,
                Err(step) => Err(step),
            }
        });
        if ready.is_none() {
            self.count_done(1, size);
            return Ok(());
        }
        self.unpack_file(plan, item, &at).await?;
        self.count_done(1, size);
        Ok(())
    }

    /// The conflict and the copy of one file; a password asked for where it needs one.
    async fn unpack_file(
        &mut self,
        plan: &mut Plan,
        item: &Item,
        at: &VfsPath,
    ) -> Result<(), Stop> {
        let source = self.src_path(&item.path);
        let resolved = attempt!(self, at, self.resolve(item, &source, at).await);
        let (dest, mode) = match resolved {
            Some(Resolved::Write(placement)) => (placement.dest, placement.mode),
            // Only a restored job finds a file complete; an unpacking is never restored.
            Some(Resolved::Skip | Resolved::Complete) => {
                // A conflict answer skipped it.
                self.job.update(|snapshot| snapshot.skipped += 1);
                return Ok(());
            }
            None => return Ok(()),
        };
        self.progress.begin_file(source.clone(), item.entry.size);
        let start = self.progress.bytes_done;
        loop {
            self.progress.restart_file(start);
            let error = match self.copy_one(item, &dest, mode).await {
                Ok(_) => break,
                Err(Step::Stop(stop)) => return Err(stop),
                Err(Step::Fail(error)) => error,
            };
            if needs_password(&error) && !plan.declined {
                if let Some(typed) = self
                    .ask_password(&plan.archive, plan.password.is_some())
                    .await?
                {
                    plan.fs =
                        self.archive_fs(&plan.outer, &plan.archive, plan.format, Some(&typed));
                    plan.password = Some(typed);
                    self.src = Arc::clone(&plan.fs) as Arc<dyn Vfs>;
                    continue;
                }
                plan.declined = true;
            }
            if needs_password(&error) {
                let failure = Failure::new(&source, &error);
                self.job.update(|snapshot| snapshot.failures.push(failure));
                break;
            }
            if self.failed(&source, error).await? == ErrorChoice::Skip {
                break;
            }
        }
        self.progress.bytes_done = start;
        Ok(())
    }
}

/// Refuses writing through a link that stands where the file goes.
async fn not_a_link(dst: &dyn Vfs, dest: &Path) -> Result<(), Step> {
    match dst.stat(dest).await {
        Ok(entry) if matches!(entry.kind, EntryKind::Symlink { .. }) => {
            Err(OpsError::UnsafeEntry {
                path: dest.to_path_buf(),
                reason: "a link in the target folder",
            }
            .into())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
