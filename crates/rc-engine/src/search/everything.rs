//! Everything (voidtools) as the source of a find-files dialog's candidates (RC-100-93), as
//! Total Commander asks it: through Everything's IPC window and `WM_COPYDATA` (the SDK 2 of
//! Everything 1.4, `everything_ipc.h`), never through `es.exe`.
//!
//! Everything only names candidates: the files whose names fit the include masks below the start
//! folders, on the machine's own drives (never a share or WSL). The walk then judges each one as
//! it would have met it ([`super::walk`]'s candidates), so exclusions, filters, depth, hidden
//! entries and the content keep rCommander's meaning. A regular expression is not handed over
//! (Everything's dialect is not Rust's); neither is a search of archives, whose insides
//! Everything does not index. When Everything cannot be asked, the dialog's search walks the
//! folders as without it.
//!
//! Here is what compiles everywhere: the search expression, the request and the parser of the
//! answer - foreign input, every offset and length checked against the buffer before it is
//! read (the fuzz target `everything_reply`). The window and the message loop are
//! `everything/windows.rs`. `files.search` never asks Everything: it is a parameter of the
//! dialog's [`crate::Engine::start_search_with`], not of [`SearchQuery`].

#[cfg(windows)]
mod windows;

use std::{
    path::{Component, MAIN_SEPARATOR, Path, PathBuf, Prefix},
    sync::Arc,
    time::Duration,
};

use super::{SearchCancel, SearchQuery};
use crate::{flat::MAX_FLAT_ENTRIES, mask::split_line, recover::LogPanic as _};

/// How long Everything may take to answer.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// Candidates taken at most: what a panel can be fed.
pub const MAX_CANDIDATES: usize = MAX_FLAT_ENTRIES;
/// The largest answer taken, in bytes.
pub const MAX_REPLY_BYTES: usize = 128 << 20;
/// The longest path a candidate may have, in UTF-16 units (Windows' long paths).
const MAX_PATH_UNITS: usize = 32_767;

// The numbers of `everything_ipc.h` (Everything SDK 2).
/// `EVERYTHING_IPC_COPYDATA_QUERY2W`: the `dwData` of a query in UTF-16.
pub const COPYDATA_QUERY2W: usize = 18;
/// `EVERYTHING_IPC_QUERY2_REQUEST_NAME`, `_PATH` and `_FULL_PATH_AND_NAME`.
const REQUEST_NAME: u32 = 0x1;
const REQUEST_PATH: u32 = 0x2;
const REQUEST_FULL_PATH_AND_NAME: u32 = 0x4;
/// `EVERYTHING_IPC_SORT_NAME_ASCENDING`: the order Everything gives without cost.
const SORT_NAME_ASCENDING: u32 = 1;
/// The fixed parts of `EVERYTHING_IPC_LIST2` and of one `EVERYTHING_IPC_ITEM2`.
const LIST2_HEADER: usize = 20;
const ITEM2_SIZE: usize = 8;

/// An Everything to ask: the one of this machine ([`system`]), or a test's stand-in.
pub trait Everything: Send + Sync {
    /// Sends `search` (a request of [`request`]'s form for at most `max` results) and waits for
    /// the answer - an `EVERYTHING_IPC_LIST2` as it arrived - until [`TIMEOUT`], or until
    /// `cancel` is set, which drops an answer still to come. Runs on a blocking thread.
    ///
    /// # Errors
    ///
    /// Everything does not run, does not answer in time, or the search was stopped.
    fn query(
        &self,
        search: &str,
        max: u32,
        cancel: &SearchCancel,
    ) -> Result<Vec<u8>, EverythingError>;
}

/// Why Everything gave no candidates.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EverythingError {
    #[error("Everything is not running")]
    NotRunning,
    #[error("Everything did not answer in time")]
    TimedOut,
    #[error("the search was stopped")]
    Cancelled,
    #[error("Everything's answer does not read: {0}")]
    Reply(&'static str),
    #[error("Everything cannot be asked: {0}")]
    System(&'static str),
}

/// The Everything of this machine: its IPC window on Windows, none elsewhere.
#[cfg(windows)]
#[must_use]
pub fn system() -> Option<Arc<dyn Everything>> {
    Some(Arc::new(windows::System))
}

/// The Everything of this machine: its IPC window on Windows, none elsewhere.
#[cfg(not(windows))]
#[must_use]
pub fn system() -> Option<Arc<dyn Everything>> {
    None
}

/// The version of the Everything running on this machine, `(major, minor)`; `None` when none
/// runs, and always off Windows. Asked off the runtime's threads: a hung Everything is waited
/// for one second at most.
pub async fn detect() -> Option<(u32, u32)> {
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(windows::version)
            .await
            .log_panic("search.everything")
            .ok()
            .flatten()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Whether an Everything of `version` answers the query rCommander sends (`QUERY2`, 1.4 on).
#[must_use]
pub fn answers(version: (u32, u32)) -> bool {
    version >= (1, 4)
}

/// Whether Everything indexes the file system of `path`: a drive of the machine (`C:\`), never a
/// share or WSL. Off Windows, where only a test's stand-in answers, any absolute path.
pub(crate) fn indexed(path: &Path) -> bool {
    if cfg!(windows) {
        matches!(
            path.components().next(),
            Some(Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        )
    } else {
        path.is_absolute()
    }
}

/// The Everything search for the include masks of `query` below `roots`; `None` when Everything
/// is not to be asked: a regular expression, a start folder or a mask it cannot take. The
/// exclusions stay out - the walk applies them - so the candidates are never fewer than the hits.
#[must_use]
pub fn expression(query: &SearchQuery, roots: &[PathBuf]) -> Option<String> {
    if query.name_regex || roots.is_empty() {
        return None;
    }
    let mut folders = Vec::with_capacity(roots.len());
    for root in roots {
        let mut folder = root.to_str()?.trim_end_matches(['\\', '/']).to_owned();
        folder.push(MAIN_SEPARATOR);
        folders.push(quoted(&folder)?);
    }
    let mut masks = Vec::new();
    for token in split_line(&query.name).0 {
        // `x.*` also takes `x`, as rCommander's masks do; `*.*` reads as `*`.
        let mask = match token.text.strip_suffix(".*") {
            Some(stem) => format!("{stem}*"),
            None => token.text,
        };
        if mask.chars().all(|c| c == '*') {
            masks.clear();
            break;
        }
        masks.push(quoted(&mask)?);
    }
    let mut search = group(&folders);
    if !masks.is_empty() {
        search.push(' ');
        search.push_str(&group(&masks));
    }
    Some(search)
}

/// `term` in double quotes, so Everything takes its spaces and operators as text; control
/// characters dropped. `None` for a term with a quote, which Everything cannot take.
fn quoted(term: &str) -> Option<String> {
    let clean: String = term.chars().filter(|c| !c.is_control()).collect();
    if clean.is_empty() || clean.contains('"') {
        return None;
    }
    Some(format!("\"{clean}\""))
}

/// One term, or several in Everything's group as alternatives.
fn group(terms: &[String]) -> String {
    match terms {
        [one] => one.clone(),
        _ => format!("<{}>", terms.join("|")),
    }
}

/// An `EVERYTHING_IPC_QUERY2` with its UTF-16 search, as `WM_COPYDATA` carries it: the answer
/// goes to the window `reply_window` (32 bits suffice, the SDK says, also on x64) as a
/// `WM_COPYDATA` whose `dwData` is `reply_id`; the full paths of at most `max` results, by name.
#[must_use]
pub fn request(reply_window: u32, reply_id: u32, search: &str, max: u32) -> Vec<u8> {
    let header = [
        reply_window,
        reply_id,
        0, // search flags: no case, no whole word, names not paths, no regex
        0, // offset
        max,
        REQUEST_FULL_PATH_AND_NAME,
        SORT_NAME_ASCENDING,
    ];
    let mut bytes = Vec::with_capacity(header.len() * 4 + (search.len() + 1) * 2);
    for value in header {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for unit in search.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// The full paths of Everything's answer (`EVERYTHING_IPC_LIST2`), at most `max`. A path that is
/// no valid UTF-16 or holds a NUL is left out.
///
/// # Errors
///
/// [`EverythingError::Reply`] for an answer that is too large, too short, names more items than
/// `max`, carries no full paths, or points outside itself.
pub fn parse_reply(bytes: &[u8], max: usize) -> Result<Vec<PathBuf>, EverythingError> {
    if bytes.len() > MAX_REPLY_BYTES {
        return Err(EverythingError::Reply("too large"));
    }
    let truncated = EverythingError::Reply("truncated");
    let count = dword(bytes, 4).ok_or_else(|| truncated.clone())?;
    let flags = dword(bytes, 12).ok_or(truncated)?;
    if flags & REQUEST_FULL_PATH_AND_NAME == 0 {
        return Err(EverythingError::Reply("no full paths"));
    }
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    if count > max {
        return Err(EverythingError::Reply("too many items"));
    }
    let items_end = count
        .checked_mul(ITEM2_SIZE)
        .and_then(|size| size.checked_add(LIST2_HEADER));
    if items_end.is_none_or(|end| end > bytes.len()) {
        return Err(EverythingError::Reply("items outside the answer"));
    }
    let mut paths = Vec::with_capacity(count);
    for index in 0..count {
        let item = LIST2_HEADER + index * ITEM2_SIZE;
        let offset = dword(bytes, item + 4).ok_or(EverythingError::Reply("item"))?;
        let mut at = usize::try_from(offset).unwrap_or(usize::MAX);
        // The texts Everything sends before the full path, when it sends them too.
        for skipped in [REQUEST_NAME, REQUEST_PATH] {
            if flags & skipped != 0 {
                at = text(bytes, at)?.1;
            }
        }
        let (units, _) = text(bytes, at)?;
        if units.contains(&0) {
            continue;
        }
        if let Ok(path) = String::from_utf16(&units) {
            paths.push(PathBuf::from(path));
        }
    }
    Ok(paths)
}

/// The little-endian `u32` at `at`.
fn dword(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let four: [u8; 4] = bytes.get(at..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(four))
}

/// The text at `at` - its length in UTF-16 units without the NUL, the units, the NUL - and the
/// offset after it.
fn text(bytes: &[u8], at: usize) -> Result<(Vec<u16>, usize), EverythingError> {
    let outside = EverythingError::Reply("text outside the answer");
    let length = dword(bytes, at).ok_or_else(|| outside.clone())?;
    let length = usize::try_from(length).unwrap_or(usize::MAX);
    if length > MAX_PATH_UNITS {
        return Err(EverythingError::Reply("text too long"));
    }
    let start = at + 4;
    let end = start + (length + 1) * 2;
    let raw = bytes.get(start..end).ok_or(outside)?;
    let mut units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    if units.pop() != Some(0) {
        return Err(EverythingError::Reply("text without its end"));
    }
    Ok((units, end))
}

/// Asks `everything` for `search` on a blocking thread: the paths of its answer.
///
/// # Errors
///
/// What [`Everything::query`] and [`parse_reply`] report.
pub(crate) async fn ask(
    everything: Arc<dyn Everything>,
    search: String,
    cancel: SearchCancel,
) -> Result<Vec<PathBuf>, EverythingError> {
    let max = u32::try_from(MAX_CANDIDATES).unwrap_or(u32::MAX);
    tokio::task::spawn_blocking(move || {
        everything
            .query(&search, max, &cancel)
            .and_then(|reply| parse_reply(&reply, MAX_CANDIDATES))
    })
    .await
    .log_panic("search.everything")
    .unwrap_or(Err(EverythingError::System("the query ended early")))
}

#[cfg(test)]
#[path = "everything_tests.rs"]
pub(crate) mod tests;
