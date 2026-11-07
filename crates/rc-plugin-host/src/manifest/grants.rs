//! The shapes of the grants a kind links: `net_http` of a secret source (RC-100-33), and
//! `net_http` and `secrets` of an inventory (RC-100-39).
//!
//! `[capabilities]` keeps every grant as raw TOML so a later kind's grant is told it is not
//! available rather than unknown (`sections.rs`); a grant is read into its shape here when the
//! kind that links it is validated, loaded or previewed.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::PluginManifest;

/// The grant name of the `http` interface.
pub const NET_HTTP: &str = "net_http";

/// `net_http = { loopback = <port> }`: plain HTTP to this machine's loopback on one port - a
/// password manager's local bridge such as `bw serve`. Targets on the network (`domains`) come
/// with the first plugin that needs them (`docs/plugins.md`, "Capabilities and rights").
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetHttp {
    pub loopback: u16,
}

impl NetHttp {
    /// The grant as the install preview names it: what the person confirms by installing.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{NET_HTTP} localhost:{}", self.loopback)
    }
}

/// The manifest's `net_http`, read into its shape; `None` when it grants none.
///
/// # Errors
///
/// The grant has another shape, or port 0.
pub fn net_http(manifest: &PluginManifest) -> Result<Option<NetHttp>> {
    let Some(raw) = manifest.capabilities.declared.get(NET_HTTP) else {
        return Ok(None);
    };
    let grant = NetHttp::deserialize(raw.clone())
        .context("[capabilities] net_http is not { loopback = <port> }")?;
    if grant.loopback == 0 {
        bail!("[capabilities] net_http loopback must name a port");
    }
    Ok(Some(grant))
}

/// `net_http = { entered = true }` (`inventory` kind, contract 0.4.0; RC-100-39): HTTPS to the
/// one address the person entered for an inventory source - a NetBox or Proxmox the manifest
/// cannot know. Read by [`net_http_entered`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct EnteredHttp {
    entered: bool,
}

/// Whether the manifest grants `net_http = { entered = true }`; `false` when it grants none.
///
/// # Errors
///
/// The grant has another shape, or says `entered = false`.
pub fn net_http_entered(manifest: &PluginManifest) -> Result<bool> {
    let Some(raw) = manifest.capabilities.declared.get(NET_HTTP) else {
        return Ok(false);
    };
    let grant = EnteredHttp::deserialize(raw.clone())
        .context("[capabilities] net_http is not { entered = true }")?;
    if !grant.entered {
        bail!("[capabilities] net_http entered must be true, or the grant left out");
    }
    Ok(true)
}

/// The grant name of the `{{secret:<slot>}}` markers.
pub const SECRETS: &str = "secrets";
/// The most slots one manifest names, and the longest slot name.
pub const MAX_SECRET_SLOTS: usize = 4;
const MAX_SLOT_CHARS: usize = 32;

/// `secrets = ["<slot>", ...]`: the slots a plugin may name in a `{{secret:<slot>}}` marker,
/// each a value the person stores in the vault for the source. Empty when it grants none.
///
/// # Errors
///
/// Not a list of strings, more than [`MAX_SECRET_SLOTS`], a name that is not lower-case letters,
/// digits and `_` (at most 32), or one named twice.
pub fn secret_slots(manifest: &PluginManifest) -> Result<Vec<String>> {
    let Some(raw) = manifest.capabilities.declared.get(SECRETS) else {
        return Ok(Vec::new());
    };
    let slots = Vec::<String>::deserialize(raw.clone())
        .context("[capabilities] secrets is not a list of slot names")?;
    if slots.is_empty() || slots.len() > MAX_SECRET_SLOTS {
        bail!("[capabilities] secrets names 1 to {MAX_SECRET_SLOTS} slots");
    }
    for (index, slot) in slots.iter().enumerate() {
        if !valid_slot(slot) || slots[..index].contains(slot) {
            bail!("[capabilities] secrets slot `{slot}` is not a plain name, or twice");
        }
    }
    Ok(slots)
}

/// A slot name: 1 to 32 of `a-z`, `0-9`, `_`.
#[must_use]
pub fn valid_slot(slot: &str) -> bool {
    !slot.is_empty()
        && slot.len() <= MAX_SLOT_CHARS
        && slot
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// One capability as the install preview names it: the grant with what it reaches, so a changed
/// port or slot list is a new permission on update.
#[must_use]
pub fn describe_capability(manifest: &PluginManifest, name: &str) -> String {
    match name {
        NET_HTTP => match (net_http(manifest), net_http_entered(manifest)) {
            (Ok(Some(grant)), _) => grant.describe(),
            (_, Ok(true)) => format!("{NET_HTTP} entered address"),
            _ => name.to_owned(),
        },
        SECRETS => match secret_slots(manifest) {
            Ok(slots) => format!("{SECRETS} {}", slots.join(", ")),
            Err(_) => name.to_owned(),
        },
        _ => name.to_owned(),
    }
}

#[cfg(test)]
#[path = "grants_tests.rs"]
mod tests;
