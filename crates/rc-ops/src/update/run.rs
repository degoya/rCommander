//! The job side of an archive update: the sources planned, the archive looked at (read-only,
//! format, room), its entries scanned, the names that exist asked about, the rewrite run as one
//! blocking task with its progress, and the new archive put in place of the old one.

use std::{
    collections::{BTreeSet, HashMap},
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use rc_vfs::{
    ArchiveFormat, Attributes, Entry, EntryKind, VfsPath, detect_format_within, long_path,
};
use tokio::sync::watch;

use super::{Change, Kind, Old, Rewrite, Stats, needed_space, ops_error, prefix, rewrite, scan};
use crate::recover::LogPanic as _;
use crate::{
    Failure, JobState, OpsError, PackSettings,
    conflict::Decision,
    pack::{
        Group,
        write::{Feed, Member, MemberKind, panicked},
    },
    runner::{Runner, Step, Stop, attempt},
    walk::Item,
};

/// How often the new archive is tried onto the old one while the old one is still open: a reading
/// session that was let go closes its file a moment later.
const REPLACE_TRIES: u32 = 5;
const REPLACE_PAUSE: Duration = Duration::from_millis(100);

/// The archive file's size and time: a change between the start and the replacing is
/// `archive_changed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
}

/// The new entries that go in, and the folders that are already there (their contents merge).
type Settled = (Vec<(Member, Item)>, Vec<Item>);

impl Runner {
    /// Adds `change.add` under `change.under` to the archive and drops the entries
    /// `change.remove`; with `delete_sources` the added files go once the archive is replaced.
    pub(crate) async fn archive_update(&mut self, change: &Change<'_>) -> Result<(), Stop> {
        let items = if change.add.is_empty() {
            self.job
                .update(|snapshot| snapshot.state = JobState::Running);
            self.tick(true);
            Vec::new()
        } else if change.settings.recurse {
            self.plan(change.add).await?
        } else {
            self.plan_flat(change.add).await?
        };
        // An MCP job adds no file its read guard refuses now (RC-080-42).
        let items = self.admitted(items).await?;
        let files: Vec<&Item> = items.iter().filter(|item| item.is_file()).collect();
        let (count, bytes) = (
            files.len() as u64,
            files.iter().map(|item| item.entry.size).sum::<u64>(),
        );
        let start = self.progress.bytes_done;
        let archive = change.archive;
        let added = attempt!(self, archive, {
            self.progress.restart_file(start);
            self.update_once(change, &items).await
        });
        self.progress.bytes_done = start;
        self.count_done(count, bytes);
        if let Some(added) = added
            && change.settings.delete_sources
        {
            self.delete_packed(&added).await?;
        }
        Ok(())
    }

    /// One try at the update; the items added (and the folders merged), for a move.
    async fn update_once(
        &mut self,
        change: &Change<'_>,
        items: &[Item],
    ) -> Result<Vec<Item>, Step> {
        let archive = change.archive;
        let path = &archive.path;
        let stamp = self.stamp(archive).await?;
        let format = detect_format_within(&self.dst, path, self.archive.limits)
            .await?
            .ok_or_else(|| OpsError::NotAnArchive { path: path.clone() })?;
        let kind = Kind::of(format)
            .ok_or_else(|| OpsError::ArchiveNotModifiable { path: path.clone() })?;
        let named = self.named(change, kind, items)?;
        let added: u64 = named
            .iter()
            .filter(|(member, _)| member.kind == MemberKind::File)
            .map(|(member, _)| member.size)
            .sum();
        let needed = needed_space(stamp.size, added);
        let dir = path.parent().unwrap_or(Path::new("/"));
        if let Some(free) = self.dst.free_space(dir).await?
            && free < needed
        {
            return Err(OpsError::NotEnoughSpace {
                path: path.clone(),
                needed,
                free,
            }
            .into());
        }
        let old = long_path(path);
        let limits = self.archive.limits;
        let scanned = {
            let old = old.clone();
            tokio::task::spawn_blocking(move || scan(kind, &old, &limits)).await
        };
        let olds = match scanned.log_panic("ops.archive_update_scan") {
            Ok(Ok(olds)) => olds,
            Ok(Err(error)) => return Err(ops_error(error, path).into()),
            Err(_) => return Err(ops_error(panicked(), path).into()),
        };
        let mut remove: BTreeSet<u64> = change.remove.iter().copied().collect();
        let (members, merged) = self
            .settle_names(archive, format, &olds, &mut remove, named)
            .await?;
        let (members, mut added_items): (Vec<Member>, Vec<Item>) = members.into_iter().unzip();
        let temp = crate::temp_name::pack_path(&old, self.job.id);
        // A run that ended without cleaning up (a crash) may have left one under this job's name.
        remove_temp(&temp).await;
        let job = Rewrite {
            kind,
            old: old.clone(),
            temp: temp.clone(),
            remove,
            members,
            level: u32::from(change.settings.level.min(9)),
            solid: change.settings.solid,
            limits,
            verify: change.settings.verify,
        };
        let (written, grown) = self.run_rewrite(job).await;
        let failed = match written {
            Err(error) => Some(ops_error(error, path)),
            Ok(_) => match self.stamp(archive).await {
                Ok(now) if now == stamp => self.replace(archive, &temp, &old).await.err(),
                // Another job or program changed it: the new archive is thrown away.
                _ => Some(OpsError::ArchiveChanged { path: path.clone() }),
            },
        };
        if let Some(error) = failed {
            remove_temp(&temp).await;
            if self.job.control.is_cancelled() {
                return Err(Step::Stop(Stop::Cancelled));
            }
            return Err(error.into());
        }
        added_items = self.without_grown(added_items, &grown);
        added_items.extend(merged);
        Ok(added_items)
    }

    /// The archive file's size and time, refusing a folder and a read-only file.
    async fn stamp(&self, archive: &VfsPath) -> Result<Stamp, Step> {
        let path = &archive.path;
        let entry = self.dst.stat(path).await?;
        if entry.is_dir_like() {
            return Err(OpsError::KindMismatch { path: path.clone() }.into());
        }
        if entry.attributes.readonly {
            return Err(OpsError::ArchiveFileReadOnly { path: path.clone() }.into());
        }
        Ok(Stamp {
            size: entry.size,
            modified: entry.modified,
        })
    }

    /// The new entries with their names inside the archive, below `change.under`; a name that is
    /// not plain is a failure of its own.
    fn named(
        &mut self,
        change: &Change<'_>,
        kind: Kind,
        items: &[Item],
    ) -> Result<Vec<(Member, Item)>, Step> {
        let prefix = prefix(change.under)?;
        let settings = PackSettings {
            format: kind.pack_format(),
            level: change.settings.level.min(9),
            solid: change.settings.solid,
            recurse: change.settings.recurse,
            keep_paths: change.settings.keep_paths,
            store_links: change.settings.store_links,
            ..PackSettings::default()
        };
        let group = Group {
            dest: change.archive.path.clone(),
            items: items.to_vec(),
        };
        let mut named = Vec::new();
        for (mut member, item) in self.members(&group, &settings) {
            member.name = format!("{prefix}{}", member.name);
            if let Err(error) = crate::check_target(Path::new(&member.name), true) {
                let failure = Failure::new(&self.src_path(&item.path), &error);
                self.job.update(|snapshot| snapshot.failures.push(failure));
                continue;
            }
            named.push((member, item));
        }
        Ok(named)
    }

    /// The new entries whose names exist in the archive: a file asks (overwrite drops the old
    /// one, skip leaves the new one out, cancel stops), a folder merges into the folder there,
    /// a file meeting a folder (or the other way round) is a failure.
    async fn settle_names(
        &mut self,
        archive: &VfsPath,
        format: ArchiveFormat,
        olds: &[Old],
        remove: &mut BTreeSet<u64>,
        named: Vec<(Member, Item)>,
    ) -> Result<Settled, Step> {
        let mut by_name: HashMap<String, Vec<&Old>> = HashMap::new();
        for old in olds.iter().filter(|old| !remove.contains(&old.index)) {
            by_name
                .entry(old.name.to_lowercase())
                .or_default()
                .push(old);
        }
        let mut members = Vec::with_capacity(named.len());
        let mut merged = Vec::new();
        for (member, item) in named {
            let Some(found) = by_name.get(&member.name.to_lowercase()) else {
                members.push((member, item));
                continue;
            };
            let is_dir = member.kind == MemberKind::Dir;
            if is_dir && found.iter().all(|old| old.dir) {
                merged.push(item);
                continue;
            }
            let Some(old) = found.iter().find(|old| !old.dir).filter(|_| !is_dir) else {
                let error = OpsError::KindMismatch {
                    path: PathBuf::from(&member.name),
                };
                let failure = Failure::new(&self.src_path(&item.path), &error);
                self.job.update(|snapshot| snapshot.failures.push(failure));
                continue;
            };
            let there = old_entry(old);
            let target = VfsPath::archive(archive.clone(), format, format!("/{}", old.name));
            let source = self.src_path(&item.path);
            match self
                .conflict((&source, &item.entry), (&target, &there), false)
                .await?
            {
                Decision::Overwrite => {
                    remove.extend(found.iter().map(|old| old.index));
                    members.push((member, item));
                }
                Decision::Cancel => return Err(Step::Stop(Stop::Cancelled)),
                // Inside an archive a new file is not renamed: it stays out.
                _ => {
                    self.job.update(|snapshot| snapshot.skipped += 1);
                }
            }
        }
        Ok((members, merged))
    }

    /// Runs the rewrite as a blocking task, its progress followed; the sources that grew while
    /// they were read.
    async fn run_rewrite(&mut self, job: Rewrite) -> (io::Result<Stats>, Vec<PathBuf>) {
        let (bytes, mut seen) = watch::channel(0_u64);
        let feed = match Feed::new(Arc::clone(&self.src), self.job.control.clone(), bytes) {
            Ok(feed) => feed,
            Err(error) => return (Err(error), Vec::new()),
        };
        let current = feed.current();
        let grown = feed.grown();
        let start = self.progress.bytes_done;
        let mut task = tokio::task::spawn_blocking(move || rewrite(&job, &feed));
        let joined = loop {
            tokio::select! {
                joined = &mut task => break joined,
                changed = seen.changed() => {
                    if changed.is_err() {
                        break (&mut task).await;
                    }
                    let read = *seen.borrow_and_update();
                    self.progress.bytes_done = start + read;
                    if let Some(member) = current.get() {
                        let at = self.src_path(&member.path);
                        self.progress.begin_file(at, member.size);
                        self.progress.file_bytes_done = read.saturating_sub(member.from);
                    }
                    self.tick(false);
                }
            }
        };
        let written = match joined.log_panic("ops.archive_update") {
            Ok(written) => written,
            Err(_) => Err(panicked()),
        };
        (written, grown.take())
    }

    /// Lets go of the archive and renames `temp` onto it (`old`, its long form); a file still
    /// open elsewhere after a few tries is `archive_in_use`.
    async fn replace(&self, archive: &VfsPath, temp: &Path, old: &Path) -> Result<(), OpsError> {
        if let Some(hook) = &self.archive_release {
            hook.release(archive);
        }
        let mut tries = 0;
        loop {
            match tokio::fs::rename(temp, old).await {
                Ok(()) => return Ok(()),
                Err(error) if in_use(&error) => {
                    tries += 1;
                    if tries >= REPLACE_TRIES {
                        return Err(OpsError::ArchiveInUse {
                            path: archive.path.clone(),
                        });
                    }
                    tokio::time::sleep(REPLACE_PAUSE).await;
                }
                Err(error) => return Err(OpsError::from_io(error, &archive.path)),
            }
        }
    }
}

/// Whether renaming onto a file failed because it is open elsewhere: Windows' sharing violation
/// or access denied.
fn in_use(error: &io::Error) -> bool {
    /// `ERROR_SHARING_VIOLATION`, `ERROR_LOCK_VIOLATION`.
    const SHARING: [i32; 2] = [32, 33];
    error.kind() == io::ErrorKind::PermissionDenied
        || (cfg!(windows)
            && error
                .raw_os_error()
                .is_some_and(|code| SHARING.contains(&code)))
}

/// An old entry as the conflict question shows it.
fn old_entry(old: &Old) -> Entry {
    let name = old.name.rsplit('/').next().unwrap_or(&old.name).to_owned();
    Entry {
        name,
        kind: EntryKind::File,
        size: old.size,
        modified: old.modified,
        created: None,
        accessed: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

/// Removes an unfinished new archive; one that cannot be removed is logged.
async fn remove_temp(temp: &Path) {
    if let Err(left) = tokio::fs::remove_file(temp).await
        && left.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(
            path = %temp.display(),
            "an unfinished archive could not be removed: {left}"
        );
    }
}
