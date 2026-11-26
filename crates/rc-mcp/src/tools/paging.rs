//! Pages of a list tool's answer (RC-110-61): `list_dir`, `archives_list` and
//! `connections_list` answer at most `limit` items (1000 when omitted, never more) with a
//! `next_cursor` and `truncated` where more follow, instead of a folder of a million entries in
//! one answer of hundreds of megabytes.
//!
//! The listing a page was cut from is kept for the next one in the file search's store
//! (`rc_engine::search::pages`): one listing, not one per page, and the pages of one listing
//! never repeat or skip an entry, whatever changes in the folder meanwhile. A kept listing goes
//! after five minutes, or when eight newer ones are kept; a cursor whose listing went lists
//! again and goes on at its offset. A cursor names the call it was handed out for (its tool and
//! arguments) and the listing (the folder): another call's cursor, and one for a folder that
//! another one stands in for by now, is refused with `invalid_params`.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, Mutex},
    time::Instant,
};

use rc_engine::{
    ConnectionTree,
    search::pages::{PageKey, Pages},
};
use rmcp::model::CallToolResult;

use super::{archives::ArchiveListing, panels::DirListing};
use crate::{answer::error_result, recover::LockOrRecover as _};

/// Items per page when a call names no limit, and the most it may name (the parameter types'
/// `range`).
pub(crate) const PAGE: usize = 1000;

/// The listings every list tool keeps between its pages; one per server, shared by its sessions.
#[derive(Default)]
pub(crate) struct ListPages {
    pub(crate) dirs: Kept<DirListing>,
    pub(crate) archives: Kept<ArchiveListing>,
    pub(crate) connections: Kept<ConnectionTree>,
}

/// The listings of one kind kept between pages, each with the print of what it lists.
pub(crate) struct Kept<T> {
    pages: Mutex<Pages<(u64, Arc<T>)>>,
}

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Self {
            pages: Mutex::new(Pages::default()),
        }
    }
}

/// Where a call's page starts.
pub(crate) struct Start<T> {
    pub(crate) offset: usize,
    /// The listing the previous page kept; `None`: list again, and go on at `offset`.
    pub(crate) kept: Option<Arc<T>>,
    /// What the cursor's listing listed: a fresh listing has to list the same.
    listing: Option<u64>,
}

/// One page: the items `from..to` of the listing, and the cursor of the next where more follow.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Page {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) next_cursor: Option<String>,
}

impl Page {
    /// Whether the listing goes on past this page.
    pub(crate) fn truncated(&self) -> bool {
        self.next_cursor.is_some()
    }
}

/// A cursor as handed out: `<offset>-<request>-<listing>-<id>`, the prints and the kept
/// listing's id in hex.
struct Cursor {
    offset: usize,
    request: u64,
    listing: u64,
    id: u64,
}

impl<T> Kept<T> {
    /// Where the page of the call `request` starts: at its `cursor`, with the listing kept for
    /// it where it still is; at the first item without one.
    pub(crate) fn start(
        &self,
        request: u64,
        cursor: Option<&str>,
    ) -> Result<Start<T>, CallToolResult> {
        let Some(cursor) = cursor else {
            return Ok(Start {
                offset: 0,
                kept: None,
                listing: None,
            });
        };
        let Some(cursor) = parse(cursor).filter(|cursor| cursor.request == request) else {
            return Err(error_result(
                "invalid_params",
                "the cursor belongs to another listing; call again without cursor, or with the \
                 next_cursor this call's previous page answered",
            ));
        };
        let kept = self.pages.lock_or_recover("mcp.list_pages").take(
            cursor.id,
            key(request, cursor.offset),
            Instant::now(),
        );
        Ok(Start {
            offset: cursor.offset,
            kept: kept
                .filter(|(listing, _)| *listing == cursor.listing)
                .map(|(_, items)| items),
            listing: Some(cursor.listing),
        })
    }

    /// The page of `items`, `len` of them, that `start` asks for, at most `limit` long; the
    /// listing kept for the next page where more follow. `listing` is the print of what `items`
    /// list: a cursor's listing that went must be listed again from the same place.
    pub(crate) fn page(
        &self,
        request: u64,
        listing: u64,
        start: &Start<T>,
        limit: usize,
        (items, len): (&Arc<T>, usize),
    ) -> Result<Page, CallToolResult> {
        if start.listing.is_some_and(|expected| expected != listing) {
            return Err(error_result(
                "invalid_params",
                "the cursor belongs to a listing of another folder; call again without cursor",
            ));
        }
        let from = start.offset.min(len);
        let to = from.saturating_add(limit.clamp(1, PAGE)).min(len);
        let next_cursor = (to < len).then(|| {
            let id = self.pages.lock_or_recover("mcp.list_pages").keep(
                key(request, to),
                (listing, Arc::clone(items)),
                Instant::now(),
            );
            format!("{to}-{request:016x}-{listing:016x}-{id:x}")
        });
        Ok(Page {
            from,
            to,
            next_cursor,
        })
    }
}

/// A print of `value` that stays the same while the server runs: what a cursor is bound to.
pub(crate) fn print(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn key(request: u64, offset: usize) -> PageKey {
    PageKey {
        fingerprint: request,
        guarded: false,
        offset,
    }
}

fn parse(cursor: &str) -> Option<Cursor> {
    let mut parts = cursor.split('-');
    let offset = parts.next()?.parse().ok()?;
    let mut hex = || u64::from_str_radix(parts.next()?, 16).ok();
    let (request, listing, id) = (hex()?, hex()?, hex()?);
    parts.next().is_none().then_some(Cursor {
        offset,
        request,
        listing,
        id,
    })
}

#[cfg(test)]
#[path = "paging_tests.rs"]
mod tests;
