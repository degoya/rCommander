//! What the links of an FTP session point at (RC-080-46).
//!
//! A listing either looks its links up itself ([`FtpConfig::resolve_links`]: at most
//! [`MAX_LINK_PROBES`], `CWD` and `SIZE` each, after the transfer) or leaves them unresolved and
//! notes them here. A `stat` of a noted link - entering it, opening it, copying it - learns what
//! it points at, and the next listing of its folder shows it so: a link to a folder as one. What
//! is kept is bounded ([`MAX_KNOWN`]) and lives as long as the session.
//!
//! [`FtpConfig::resolve_links`]: crate::ftp::FtpConfig::resolve_links

use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

use rc_vfs::{Entry, EntryKind, TargetKind};

use crate::ftp::{encoding::Wire, error::Failure, pool::Lease};

/// Links whose target kind a listing looks up at most; the rest stay unresolved.
const MAX_LINK_PROBES: usize = 64;

/// Links a session keeps track of at most; one more starts over.
const MAX_KNOWN: usize = 4096;

/// The links listings showed, by their FTP path, with what they point at once known.
#[derive(Debug, Default)]
pub(super) struct Links(Mutex<HashMap<String, Option<TargetKind>>>);

impl Links {
    /// Fills in the links of `entries`, listed in `dir`, from what is known, and notes the
    /// others for a later look-up.
    pub(super) fn fill(&self, dir: &str, entries: &mut [Entry]) {
        let mut known = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        for entry in entries.iter_mut().filter(|entry| unresolved_link(entry)) {
            let path = crate::wire::child(dir, &entry.name);
            match known.get(&path).copied() {
                Some(Some(kind)) => {
                    if let EntryKind::Symlink { target_kind, .. } = &mut entry.kind {
                        *target_kind = Some(kind);
                    }
                }
                Some(None) => {}
                None => {
                    if known.len() >= MAX_KNOWN {
                        known.clear();
                    }
                    known.insert(path, None);
                }
            }
        }
    }

    /// Whether a listing showed `path` as a link.
    pub(super) fn listed(&self, path: &str) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(path)
    }

    /// Notes that the link `path` points at `kind`.
    pub(super) fn learn(&self, path: &str, kind: TargetKind) {
        let mut known = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if known.len() >= MAX_KNOWN && !known.contains_key(path) {
            known.clear();
        }
        known.insert(path.to_owned(), Some(kind));
    }
}

/// A link whose target kind is not known yet.
pub(super) fn unresolved_link(entry: &Entry) -> bool {
    matches!(
        entry.kind,
        EntryKind::Symlink {
            target_kind: None,
            ..
        }
    )
}

/// What the link at `path` points at: `CWD` succeeds on a directory, `SIZE` on a file; neither
/// on a dangling link (`None`). A broken connection is the error.
pub(super) async fn link_kind(
    lease: &mut Lease,
    path: &str,
) -> Result<Option<TargetKind>, Failure> {
    match lease.cwd(path).await {
        Ok(()) => Ok(Some(TargetKind::Dir)),
        Err(failure @ Failure::Broken(_)) => Err(failure),
        Err(Failure::Reply { .. }) => match lease.size(path).await {
            Ok(_) => Ok(Some(TargetKind::File)),
            Err(failure @ Failure::Broken(_)) => Err(failure),
            Err(Failure::Reply { .. }) => Ok(None),
        },
    }
}

/// Fills in what the links of a listing in `dir` point at, and notes it in `links`. A link
/// whose name cannot be sent stays unresolved.
pub(super) async fn resolve_links(
    lease: &mut Lease,
    wire: Wire,
    dir: &str,
    entries: &mut [Entry],
    links: &Links,
) {
    let probed = entries
        .iter_mut()
        .filter(|entry| unresolved_link(entry) && (wire == Wire::Utf8 || entry.name.is_ascii()))
        .take(MAX_LINK_PROBES);
    for entry in probed {
        let path = crate::wire::child(dir, &entry.name);
        let Ok(kind) = link_kind(lease, &path).await else {
            return;
        };
        if let Some(kind) = kind {
            links.learn(&path, kind);
        }
        if let EntryKind::Symlink { target_kind, .. } = &mut entry.kind {
            *target_kind = kind;
        }
    }
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
