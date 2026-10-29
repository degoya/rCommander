//! Reading the tree: listings, `stat` and the probes that type a refusal.
//!
//! A panel's listing streams ([`FtpVfs::list_dir_batches`], RC-080-46): the entries are sent as
//! the lines arrive. Links are looked up lazily by default - listed as they are, resolved when
//! one is stat-ed, remembered for the next listing (`links.rs`); with
//! [`crate::ftp::FtpConfig::resolve_links`] they are held back until the listing ended and the
//! connection is free to look up what they point at, then sent last.

use std::{path::Path, sync::atomic::Ordering, time::SystemTime};

use rc_vfs::{Attributes, Entry, EntryKind, Result, TargetKind, VfsError};
use tokio::sync::mpsc::Sender;

use super::{
    FtpVfs,
    links::{link_kind, resolve_links, unresolved_link},
    not_found,
};
use crate::ftp::{
    error::Failure,
    listing::{Dialect, parse_facts, parse_line},
    pool::{Lease, Listing},
    time::parse_stamp,
};

/// An entry built from the name alone, for what `SIZE`/`CWD` reveal.
fn bare_entry(name: String, kind: EntryKind, size: u64, modified: Option<SystemTime>) -> Entry {
    Entry {
        hidden: name.starts_with('.'),
        name,
        kind,
        size,
        modified,
        created: None,
        attributes: Attributes::default(),
        owner: None,
    }
}

impl FtpVfs {
    pub(super) async fn list_dir(&self, dir: &Path) -> Result<Vec<Entry>> {
        let wire_dir = self.wire_path(dir)?;
        let mut lease = self.lease(dir).await?;
        // `CWD` first: it fails on a file, where `LIST file` would list the file itself, and
        // `LIST` without an argument is spared the server's own argument parsing.
        if let Err(failure) = lease.cwd(&wire_dir).await {
            drop(lease);
            return Err(self.explain_list(dir, failure).await);
        }
        let (bytes, dialect) = if self.pool.info.mlst {
            match lease.listing("MLSD").await {
                Ok(bytes) => (Ok(bytes), Dialect::Mlsd),
                Err(failure) if failure.not_implemented() => {
                    (self.list(&mut lease).await, Dialect::List)
                }
                Err(failure) => (Err(failure), Dialect::Mlsd),
            }
        } else {
            (self.list(&mut lease).await, Dialect::List)
        };
        let bytes = bytes.map_err(|failure| failure.into_vfs(dir))?;

        let now = SystemTime::now();
        let wire = self.pool.wire;
        let mut entries: Vec<Entry> = bytes
            .split(|&byte| byte == b'\n')
            .filter_map(|line| parse_line(&wire.decode(line), dialect, now))
            .collect();
        if self.pool.config.resolve_links {
            resolve_links(&mut lease, wire, &wire_dir, &mut entries, &self.links).await;
        } else {
            self.links.fill(&wire_dir, &mut entries);
        }
        Ok(entries)
    }

    /// [`Self::list_dir`] sent to `batches` as it arrives: every entry with the chunk it came in,
    /// the links too (unless the listing resolves them: then they come last, resolved). A
    /// receiver that is gone stops the listing, without an error.
    pub(super) async fn list_dir_batches(
        &self,
        dir: &Path,
        batches: Sender<Vec<Entry>>,
    ) -> Result<()> {
        let wire_dir = self.wire_path(dir)?;
        let mut lease = self.lease(dir).await?;
        if let Err(failure) = lease.cwd(&wire_dir).await {
            drop(lease);
            return Err(self.explain_list(dir, failure).await);
        }
        let (mut listing, dialect) = self
            .open_listing(&mut lease)
            .await
            .map_err(|failure| failure.into_vfs(dir))?;
        let now = SystemTime::now();
        let wire = self.pool.wire;
        let parse = |lines: &[u8]| -> Vec<Entry> {
            lines
                .split(|&byte| byte == b'\n')
                .filter_map(|line| parse_line(&wire.decode(line), dialect, now))
                .collect()
        };
        let resolving = self.pool.config.resolve_links;
        let mut links = Vec::new();
        let mut partial = Vec::new();
        loop {
            let chunk = lease
                .listing_chunk(&mut listing)
                .await
                .map_err(|failure| failure.into_vfs(dir))?;
            let done = chunk.is_none();
            let entries = match chunk {
                Some(chunk) => {
                    partial.extend_from_slice(&chunk);
                    let complete = partial
                        .iter()
                        .rposition(|&byte| byte == b'\n')
                        .map_or(0, |at| at + 1);
                    parse(&partial.drain(..complete).collect::<Vec<u8>>())
                }
                None => parse(&std::mem::take(&mut partial)),
            };
            let ready = if resolving {
                let (held, ready): (Vec<Entry>, Vec<Entry>) =
                    entries.into_iter().partition(unresolved_link);
                links.extend(held);
                ready
            } else {
                let mut ready = entries;
                self.links.fill(&wire_dir, &mut ready);
                ready
            };
            if !ready.is_empty() && batches.send(ready).await.is_err() {
                // The transfer is left unfinished: the connection is not reused.
                lease.discard();
                return Ok(());
            }
            if done {
                break;
            }
        }
        lease
            .listing_end(listing)
            .await
            .map_err(|failure| failure.into_vfs(dir))?;
        resolve_links(&mut lease, wire, &wire_dir, &mut links, &self.links).await;
        if !links.is_empty() {
            // A receiver that is gone wants nothing more.
            let _ = batches.send(links).await;
        }
        Ok(())
    }

    /// `MLSD` where offered and known, else `LIST` ([`Self::list_open`]), opened.
    async fn open_listing(
        &self,
        lease: &mut Lease,
    ) -> std::result::Result<(Listing, Dialect), Failure> {
        if self.pool.info.mlst {
            match lease.listing_open("MLSD").await {
                Ok(listing) => return Ok((listing, Dialect::Mlsd)),
                Err(failure) if failure.not_implemented() => {}
                Err(failure) => return Err(failure),
            }
        }
        Ok((self.list_open(lease).await?, Dialect::List))
    }

    /// [`Self::list`], opened to be read as it arrives.
    async fn list_open(&self, lease: &mut Lease) -> std::result::Result<Listing, Failure> {
        if self.pool.list_all.load(Ordering::Relaxed) {
            match lease.listing_open("LIST -a").await {
                Err(failure) if failure.not_implemented() || failure.reply_code() == Some(501) => {
                    self.pool.list_all.store(false, Ordering::Relaxed);
                }
                opened => return opened,
            }
        }
        lease.listing_open("LIST").await
    }

    /// `LIST -a` on a Unix server, for the dot files; plain `LIST` where the server refuses the
    /// option (`500`, `501`, `502`, `504`), from then on.
    async fn list(&self, lease: &mut Lease) -> std::result::Result<Vec<u8>, Failure> {
        if self.pool.list_all.load(Ordering::Relaxed) {
            match lease.listing("LIST -a").await {
                Err(failure) if failure.not_implemented() || failure.reply_code() == Some(501) => {
                    self.pool.list_all.store(false, Ordering::Relaxed);
                }
                listed => return listed,
            }
        }
        lease.listing("LIST").await
    }

    /// Why `CWD` into `dir` failed: missing, not a directory, or what the server said.
    async fn explain_list(&self, dir: &Path, failure: Failure) -> VfsError {
        match self.probe(dir).await {
            Ok(None) => not_found(dir),
            Ok(Some(entry)) if !entry.is_dir_like() => VfsError::NotADirectory {
                path: dir.to_path_buf(),
            },
            Ok(Some(_)) | Err(_) => failure.into_vfs(dir),
        }
    }

    /// `MLST` where offered; otherwise `SIZE` and `MDTM` for a file, `CWD` for a directory.
    pub(super) async fn stat_path(&self, path: &Path) -> Result<Entry> {
        let wire = self.wire_path(path)?;
        let name = path.file_name().map_or_else(
            || "/".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        if wire == "/" {
            return Ok(bare_entry(name, EntryKind::Dir, 0, None));
        }
        let mut lease = self.lease(path).await?;
        if self.pool.info.mlst {
            match lease.mlst(&wire).await {
                Ok(line) => {
                    if let Some(mut entry) = parse_facts(&line) {
                        if unresolved_link(&entry) {
                            // Looked at, a link says what it points at (RC-080-46).
                            let kind = link_kind(&mut lease, &wire)
                                .await
                                .map_err(|failure| failure.into_vfs(path))?;
                            if let EntryKind::Symlink { target_kind, .. } = &mut entry.kind {
                                *target_kind = kind;
                            }
                            if let Some(kind) = kind {
                                self.links.learn(&wire, kind);
                            }
                        }
                        return Ok(Entry {
                            hidden: name.starts_with('.'),
                            name,
                            ..entry
                        });
                    }
                }
                Err(failure @ Failure::Broken(_)) => return Err(failure.into_vfs(path)),
                Err(failure) if failure.not_implemented() => {}
                Err(_) => return Err(not_found(path)),
            }
        }
        match lease.size(&wire).await {
            Ok(size) => {
                let modified = match lease.mdtm(&wire).await {
                    Ok(stamp) => parse_stamp(&stamp),
                    Err(_) => None,
                };
                self.learn_if_listed(&wire, TargetKind::File);
                return Ok(bare_entry(name, EntryKind::File, size, modified));
            }
            Err(failure @ Failure::Broken(_)) => return Err(failure.into_vfs(path)),
            Err(_) => {}
        }
        match lease.cwd(&wire).await {
            Ok(()) => {
                self.learn_if_listed(&wire, TargetKind::Dir);
                Ok(bare_entry(name, EntryKind::Dir, 0, None))
            }
            Err(failure @ Failure::Broken(_)) => Err(failure.into_vfs(path)),
            Err(_) => Err(not_found(path)),
        }
    }

    /// Notes what `path` is when a listing showed it as a link: `SIZE` and `CWD` follow links.
    fn learn_if_listed(&self, path: &str, kind: TargetKind) {
        if self.links.listed(path) {
            self.links.learn(path, kind);
        }
    }

    /// What is at `path`, `None` when nothing is; other failures pass through.
    pub(super) async fn probe(&self, path: &Path) -> Result<Option<Entry>> {
        match self.stat_path(path).await {
            Ok(entry) => Ok(Some(entry)),
            Err(VfsError::NotFound { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
#[path = "inspect_tests.rs"]
mod tests;
