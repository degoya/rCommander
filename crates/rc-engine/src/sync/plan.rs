//! The walk over both trees that builds a [`SyncPlan`].

use std::{
    cmp::Ordering,
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use rc_vfs::{Entry, EntryKind, TargetKind};
use tokio_util::sync::CancellationToken;

use super::{
    ItemKind, Masks, PairState, SyncItem, SyncOptions, SyncPlan, SyncRow, proposed_action,
};
use crate::{
    EngineError,
    compare::{Side, binary, read_failure},
    read_guard,
};

const HOUR: Duration = Duration::from_secs(3600);

/// What is still to do, depth first: a directory pair to list, or a pair that is decided
/// without listing (two files, one file, or a file against a directory).
enum Work {
    Dir {
        path: PathBuf,
        left: bool,
        right: bool,
    },
    Pair {
        path: PathBuf,
        left: Option<SyncItem>,
        right: Option<SyncItem>,
    },
}

/// Compares the trees at `left` and `right` and proposes an action per row for
/// `options.mode`. Checks `cancel` before every directory and every pair of files.
///
/// # Errors
///
/// A root is not a directory or a directory cannot be listed (a plan never leaves out what it
/// could not read), a file compared by content cannot be read, or `cancel` was cancelled.
pub async fn plan(
    left: Side<'_>,
    right: Side<'_>,
    options: &SyncOptions,
    cancel: &CancellationToken,
) -> Result<SyncPlan, EngineError> {
    plan_with_lone_dirs(left, right, options, false, cancel).await
}

/// [`plan`]; with `lone_dirs` and without `options.subdirs`, a directory only one side has still
/// gets a row, `LeftOnly` or `RightOnly`, without being listed: the folders Shift+F2 selects when
/// folders take part in the selection (RC-020-08). A directory on both sides is left out as
/// before.
///
/// # Errors
///
/// As [`plan`].
pub(crate) async fn plan_with_lone_dirs(
    left: Side<'_>,
    right: Side<'_>,
    options: &SyncOptions,
    lone_dirs: bool,
    cancel: &CancellationToken,
) -> Result<SyncPlan, EngineError> {
    let walk = Walker {
        left,
        right,
        options,
        masks: Masks::parse(&options.masks),
        names: Naming::of(left, right),
        lone_dirs,
        cancel,
    };
    let mut rows: Vec<SyncRow> = Vec::new();
    let mut skipped = 0;
    let mut stack = vec![Work::Dir {
        path: PathBuf::new(),
        left: true,
        right: true,
    }];
    while let Some(work) = stack.pop() {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        match work {
            Work::Pair { path, left, right } => rows.push(walk.pair(path, left, right).await?),
            Work::Dir { path, left, right } => {
                skipped += walk.dir(path, (left, right), &mut rows, &mut stack).await?;
            }
        }
    }
    Ok(SyncPlan {
        left: left.path.clone(),
        right: right.path.clone(),
        options: options.clone(),
        rows,
        skipped,
    })
}

/// One walk over both trees: what each step needs.
struct Walker<'a> {
    left: Side<'a>,
    right: Side<'a>,
    options: &'a SyncOptions,
    masks: Masks,
    names: Naming,
    lone_dirs: bool,
    cancel: &'a CancellationToken,
}

impl Walker<'_> {
    /// The row of a pair decided without listing: two files, one file, or a file against a
    /// directory.
    async fn pair(
        &self,
        path: PathBuf,
        l: Option<SyncItem>,
        r: Option<SyncItem>,
    ) -> Result<SyncRow, EngineError> {
        let state = match (&l, &r) {
            (Some(a), Some(b)) if a.kind != b.kind => PairState::KindMismatch,
            (Some(a), Some(b)) => {
                file_state(
                    self.left,
                    self.right,
                    &path,
                    a,
                    b,
                    self.options,
                    self.cancel,
                )
                .await?
            }
            (Some(_), None) => PairState::LeftOnly,
            _ => PairState::RightOnly,
        };
        Ok(row(path, l, r, state, self.options))
    }

    /// The directory pair `path`, on the sides `has`: both listed at once, its own row (the roots
    /// have none) into `rows`, what it holds onto `stack`. Answers the entries skipped.
    async fn dir(
        &self,
        path: PathBuf,
        (has_left, has_right): (bool, bool),
        rows: &mut Vec<SyncRow>,
        stack: &mut Vec<Work>,
    ) -> Result<u64, EngineError> {
        let (left_entries, right_entries) = tokio::try_join!(
            list(self.left, &path, has_left),
            list(self.right, &path, has_right)
        )?;
        let (mut skipped, mut partial) = (0, false);
        let mut children = Vec::new();
        for (name, l, r) in self.names.pair(left_entries, right_entries) {
            let (l, r) = (l.as_ref().and_then(item), r.as_ref().and_then(item));
            if l.is_none() && r.is_none() {
                skipped += 1;
                partial = true;
                continue;
            }
            let kinds = (l.as_ref().map(|i| i.kind), r.as_ref().map(|i| i.kind));
            let dir = matches!(
                kinds,
                (Some(ItemKind::Dir), None | Some(ItemKind::Dir)) | (None, Some(ItemKind::Dir))
            );
            let mismatch = matches!(kinds, (Some(a), Some(b)) if a != b);
            let wanted = if dir {
                let lone = self.lone_dirs && (l.is_none() || r.is_none());
                (self.options.subdirs || lone) && self.masks.includes_dir(&name)
            } else {
                mismatch || self.masks.includes_file(&name)
            };
            if wanted {
                children.push((name, l, r, dir));
            } else {
                partial = true;
            }
        }
        if !path.as_os_str().is_empty() {
            rows.push(dir_row(&path, (has_left, has_right), partial, self.options));
        }
        // Files first, then directories, each by name; pushed in reverse to pop in order. The
        // keys are made once per entry, not twice per comparison.
        children.sort_by_cached_key(|(name, _, _, dir)| (*dir, name.to_lowercase(), name.clone()));
        for (name, l, r, dir) in children.into_iter().rev() {
            let path = path.join(name);
            // Without subdirectories a directory here is a lone one: a row, never listed.
            stack.push(if dir && self.options.subdirs {
                Work::Dir {
                    path,
                    left: l.is_some(),
                    right: r.is_some(),
                }
            } else {
                Work::Pair {
                    path,
                    left: l,
                    right: r,
                }
            });
        }
        Ok(skipped)
    }
}

/// The row of a directory below the roots, on the sides `has`; `partial` when entries of it
/// were left out.
fn dir_row(
    path: &Path,
    (has_left, has_right): (bool, bool),
    partial: bool,
    options: &SyncOptions,
) -> SyncRow {
    let dir_item = || SyncItem {
        kind: ItemKind::Dir,
        size: 0,
        modified: None,
    };
    let state = match (has_left, has_right) {
        (true, true) => PairState::Equal,
        (true, false) => PairState::LeftOnly,
        _ => PairState::RightOnly,
    };
    let mut made = row(
        path.to_path_buf(),
        has_left.then(dir_item),
        has_right.then(dir_item),
        state,
        options,
    );
    made.partial = partial;
    made
}

fn row(
    path: PathBuf,
    left: Option<SyncItem>,
    right: Option<SyncItem>,
    state: PairState,
    options: &SyncOptions,
) -> SyncRow {
    SyncRow {
        path,
        left,
        right,
        state,
        action: proposed_action(state, options.mode),
        partial: false,
    }
}

/// The entries of `path` below a root; none for a side that does not have the directory.
async fn list(side: Side<'_>, path: &Path, exists: bool) -> Result<Vec<Entry>, EngineError> {
    if !exists {
        return Ok(Vec::new());
    }
    let dir = if path.as_os_str().is_empty() {
        side.path.path.clone()
    } else {
        side.path.path.join(path)
    };
    Ok(side.vfs.list(&dir).await?)
}

/// An entry as a side of a row: files and links to files as files, directories as
/// directories. A link to a directory is not followed (it could loop), and devices, sockets
/// and dangling links have nothing to synchronize: `None`, counted as skipped.
fn item(entry: &Entry) -> Option<SyncItem> {
    let kind = match entry.kind {
        EntryKind::File
        | EntryKind::Symlink {
            target_kind: Some(TargetKind::File),
            ..
        } => ItemKind::File,
        EntryKind::Dir => ItemKind::Dir,
        _ => return None,
    };
    Some(SyncItem {
        kind,
        size: if kind == ItemKind::File {
            entry.size
        } else {
            0
        },
        modified: entry.modified,
    })
}

/// How names of the two sides are matched.
struct Naming {
    ignore_case: bool,
    right_spelling: bool,
}

impl Naming {
    /// As the sides' file systems compare names.
    fn of(left: Side<'_>, right: Side<'_>) -> Self {
        let left_cs = left.vfs.capabilities().case_sensitive;
        let right_cs = right.vfs.capabilities().case_sensitive;
        Self {
            ignore_case: !left_cs || !right_cs,
            // A name that differs only in case is spelled as the case-sensitive side spells it;
            // the other side finds it either way.
            right_spelling: right_cs && !left_cs,
        }
    }

    /// The entries of both sides by name: the name as the row spells it, then each side's
    /// entry. An exact match goes first, then (on a case-insensitive side) one ignoring case.
    fn pair(
        &self,
        left: Vec<Entry>,
        right: Vec<Entry>,
    ) -> Vec<(String, Option<Entry>, Option<Entry>)> {
        let mut exact: HashMap<String, usize> = HashMap::new();
        let mut folded: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, entry) in right.iter().enumerate() {
            exact.insert(entry.name.clone(), index);
            if self.ignore_case {
                folded
                    .entry(entry.name.to_lowercase())
                    .or_default()
                    .push(index);
            }
        }
        let mut right: Vec<Option<Entry>> = right.into_iter().map(Some).collect();
        let mut pairs = Vec::new();
        let mut unmatched = Vec::new();
        for entry in left {
            match exact
                .get(&entry.name)
                .and_then(|&index| right[index].take())
            {
                Some(other) => pairs.push((entry.name.clone(), Some(entry), Some(other))),
                None => unmatched.push(entry),
            }
        }
        for entry in unmatched {
            let other = folded
                .get(&entry.name.to_lowercase())
                .and_then(|indices| indices.iter().find_map(|&index| right[index].take()));
            let name = match &other {
                Some(other) if self.right_spelling => other.name.clone(),
                _ => entry.name.clone(),
            };
            pairs.push((name, Some(entry), other));
        }
        pairs.extend(
            right
                .into_iter()
                .flatten()
                .map(|entry| (entry.name.clone(), None, Some(entry))),
        );
        pairs
    }
}

/// How two files of the same path compare.
async fn file_state(
    left: Side<'_>,
    right: Side<'_>,
    path: &Path,
    l: &SyncItem,
    r: &SyncItem,
    options: &SyncOptions,
    cancel: &CancellationToken,
) -> Result<PairState, EngineError> {
    let times = if options.ignore_date {
        None
    } else {
        compare_times(l.modified, r.modified, options)
    };
    let same_size = l.size == r.size;
    let content = if options.by_content && same_size && !guarded(left, right, path).await {
        Some(same_content(left, right, path, cancel).await?)
    } else {
        None
    };
    Ok(match (content, times) {
        (Some(true), _) => PairState::Equal,
        (_, Some(Ordering::Greater)) => PairState::LeftNewer,
        (_, Some(Ordering::Less)) => PairState::RightNewer,
        (None, _) if same_size => PairState::Equal,
        _ => PairState::Different,
    })
}

/// Which time is newer, `Equal` within the tolerance (and, with `ignore_dst_hour`, one hour
/// apart within the tolerance); `None` when a side has no time.
pub(crate) fn compare_times(
    left: Option<SystemTime>,
    right: Option<SystemTime>,
    options: &SyncOptions,
) -> Option<Ordering> {
    let (left, right) = (left?, right?);
    let (apart, newer) = match left.duration_since(right) {
        Ok(apart) => (apart, Ordering::Greater),
        Err(error) => (error.duration(), Ordering::Less),
    };
    let tolerance = Duration::from_secs(options.time_tolerance_secs.into());
    if apart <= tolerance || (options.ignore_dst_hour && apart.abs_diff(HOUR) <= tolerance) {
        Some(Ordering::Equal)
    } else {
        Some(newer)
    }
}

/// Whether the read guard of an MCP dispatch keeps either file of `path` from being read; such
/// a pair is judged by size and time only, so its rows tell nothing about the contents.
async fn guarded(left: Side<'_>, right: Side<'_>, path: &Path) -> bool {
    for side in [left, right] {
        let file = side.path.join(path);
        if let Some(guard) = read_guard::current(&file.backend)
            && guard.refuses_off_thread(&file.path).await.is_some()
        {
            return true;
        }
    }
    false
}

async fn same_content(
    left: Side<'_>,
    right: Side<'_>,
    path: &Path,
    cancel: &CancellationToken,
) -> Result<bool, EngineError> {
    let (left_path, right_path) = (left.path.join(path), right.path.join(path));
    let mut left_stream = left.vfs.open_read(&left_path.path).await?;
    let mut right_stream = right.vfs.open_read(&right_path.path).await?;
    binary::first_difference(&mut left_stream, &mut right_stream, cancel)
        .await
        .map(|difference| difference.is_none())
        .map_err(|failure| read_failure(failure, &left_path, &right_path))
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
