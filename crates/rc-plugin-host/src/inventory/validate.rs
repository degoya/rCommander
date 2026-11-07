//! What an inventory answers, held to the host's rules (`docs/plugins.md`, "Validation of plugin
//! output"): the list as a whole bounded, every host checked on its own. A host breaking a rule
//! is left out and counted, never repaired into something the inventory did not say; its id is
//! kept (when it is one), so the reconciliation does not take a host it could not read for one
//! that is gone.

use std::{collections::HashSet, net::Ipv6Addr};

use rc_plugin_api::inventory::{InventoryHost, MAX_HOSTS, Protocol};
use rc_plugin_types::{Failure, FailureKind};

use crate::{
    bindings::inventory_world::exports::rcommander::plugin::inventory as wit,
    failure::{self, codes, readable},
};

/// The longest external id: handed back exactly, so it is never cut.
pub const MAX_EXTERNAL_ID_CHARS: usize = 256;
/// The longest name, user and address: a connection's own limits (`rc_db`, 255).
pub const MAX_HOST_TEXT_CHARS: usize = 255;
/// How deep a folder path goes, and the longest name of one of its folders.
pub const MAX_FOLDER_DEPTH: usize = 8;
pub const MAX_FOLDER_NAME_CHARS: usize = 128;
/// The most tags one host keeps, and the longest; further ones are dropped.
pub const MAX_TAGS: usize = 32;
pub const MAX_TAG_CHARS: usize = 64;
/// The longest note kept; a longer one is cut.
pub const MAX_NOTE_CHARS: usize = 4096;

/// The hosts of one `list`, checked.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InventoryList {
    /// The hosts that passed, each external id once.
    pub hosts: Vec<InventoryHost>,
    /// The external ids of hosts left out for breaking a rule: not gone, only unread.
    pub skipped_ids: Vec<String>,
    /// How many hosts were left out, with or without a usable id.
    pub skipped: usize,
}

/// The list `list` answered; `mask` takes out what a server may have echoed.
pub(super) fn hosts(
    raw: Vec<wit::InventoryHost>,
    mask: &dyn Fn(&str) -> String,
) -> Result<InventoryList, Failure> {
    if raw.len() > MAX_HOSTS {
        return Err(
            failure::coded(FailureKind::BadData, codes::PLUGIN_INVENTORY_TOO_LARGE)
                .with_param("count", raw.len().to_string())
                .with_param("limit", MAX_HOSTS.to_string()),
        );
    }
    let mut list = InventoryList::default();
    let mut seen = HashSet::new();
    for host in raw {
        let id = external_id(&host.external_id).map(str::to_owned);
        match (&id, checked(host, mask)) {
            (Some(id), Some(host)) if seen.insert(id.clone()) => list.hosts.push(host),
            // A second host under one id is the inventory contradicting itself: the first wins.
            (Some(_), Some(_)) => list.skipped += 1,
            (Some(id), None) => {
                list.skipped += 1;
                if seen.insert(id.clone()) {
                    list.skipped_ids.push(id.clone());
                }
            }
            (None, _) => list.skipped += 1,
        }
    }
    Ok(list)
}

/// `id` when it can be stored and matched again exactly: not empty, at most
/// [`MAX_EXTERNAL_ID_CHARS`], nothing invisible.
fn external_id(id: &str) -> Option<&str> {
    (!id.is_empty()
        && id.chars().count() <= MAX_EXTERNAL_ID_CHARS
        && !id.chars().any(rc_rename::invisible))
    .then_some(id)
}

/// One host, or `None` when it breaks a rule.
fn checked(host: wit::InventoryHost, mask: &dyn Fn(&str) -> String) -> Option<InventoryHost> {
    let name = line(&mask(&host.name), MAX_HOST_TEXT_CHARS)?;
    let address = address(&host.address)?;
    if host.port == 0 {
        return None;
    }
    let user = match host.user.as_deref().map(mask) {
        Some(user) if !user.trim().is_empty() => Some(line(&user, MAX_HOST_TEXT_CHARS)?),
        _ => None,
    };
    let folder = folder(&mask(&host.folder))?;
    let mut tags = Vec::new();
    for tag in host.tags.iter().take(MAX_TAGS) {
        let tag = mask(tag);
        if tag.trim().is_empty() {
            continue;
        }
        let tag = line(&tag, MAX_TAG_CHARS)?;
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    let note = match host.note.as_deref().map(mask) {
        Some(note) if !note.trim().is_empty() => Some(note_text(&note)?),
        _ => None,
    };
    Some(InventoryHost {
        external_id: host.external_id,
        name,
        address,
        port: host.port,
        protocol: protocol(host.protocol),
        user,
        folder,
        tags,
        note,
    })
}

/// One line of text: trimmed, not empty, at most `limit` characters, nothing invisible (control,
/// zero-width, direction characters) - a name that shows other than it reads is refused.
fn line(text: &str, limit: usize) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && text.chars().count() <= limit && !text.chars().any(rc_rename::invisible))
        .then(|| text.to_owned())
}

/// A host name or an IP address and nothing else: no scheme, user, port, path or space. An IPv6
/// address is taken bare or in brackets and kept bare.
fn address(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().count() > MAX_HOST_TEXT_CHARS {
        return None;
    }
    let bare = raw
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(raw);
    if bare.parse::<Ipv6Addr>().is_ok() {
        return Some(bare.to_owned());
    }
    // The URL standard's host parser: refuses `/`, `@`, `:`, `?`, `#`, spaces, `%` and every
    // control character, and checks an IPv4 address.
    url::Host::<String>::parse(raw).ok()?;
    raw.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        .then(|| raw.to_owned())
}

/// A folder path below the source's folder: `/`-separated names, empty for the folder itself.
/// No `.` or `..`, no `\`, no empty name between two slashes, at most [`MAX_FOLDER_DEPTH`] deep.
fn folder(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_matches('/');
    if raw.is_empty() {
        return Some(String::new());
    }
    let mut names = Vec::new();
    for name in raw.split('/') {
        let name = line(name, MAX_FOLDER_NAME_CHARS)?;
        if name == "." || name == ".." || name.contains('\\') {
            return None;
        }
        names.push(name);
    }
    (names.len() <= MAX_FOLDER_DEPTH).then(|| names.join("/"))
}

/// A note: line breaks and tabs kept, every other invisible character refused, cut at
/// [`MAX_NOTE_CHARS`].
fn note_text(note: &str) -> Option<String> {
    if note
        .chars()
        .any(|c| rc_rename::invisible(c) && !matches!(c, '\n' | '\r' | '\t'))
    {
        return None;
    }
    let note = note.trim();
    if note.chars().count() > MAX_NOTE_CHARS {
        return Some(readable(note, MAX_NOTE_CHARS));
    }
    Some(note.to_owned())
}

fn protocol(protocol: wit::Protocol) -> Protocol {
    match protocol {
        wit::Protocol::Ssh => Protocol::Ssh,
        wit::Protocol::Sftp => Protocol::Sftp,
        wit::Protocol::Ftp => Protocol::Ftp,
        wit::Protocol::Ftps => Protocol::Ftps,
    }
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
