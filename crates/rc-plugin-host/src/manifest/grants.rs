//! The shapes of the grants a kind links: `net_http` of a secret source (RC-100-33).
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
