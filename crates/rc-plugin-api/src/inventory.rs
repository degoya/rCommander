//! The native side of the `inventory-plugin` world (RC-100-39): the records of
//! `interface inventory` and the [`Inventory`] trait a server inventory's logic implements against
//! an [`InventoryHostApi`].
//!
//! An inventory reads the hosts of one system (NetBox, Proxmox, ...) and hands them to rCommander,
//! which keeps a folder of connections in step with them. It reaches its system only through
//! `http` to the address the person entered for the source, and names the API token it never
//! sees as a `{{secret:<slot>}}` marker the host fills in on the way out (`docs/plugins.md`,
//! "Inventory").

use crate::Failure;

pub use crate::secrets::HttpResponse;

/// The most hosts one `list` may answer; a longer list is refused as a whole.
pub const MAX_HOSTS: usize = 10_000;

/// The marker a request names a secret slot with: `{{secret:<slot>}}`.
pub const SECRET_MARKER_OPEN: &str = "{{secret:";

/// `{{secret:<slot>}}` for `slot`: what a plugin writes into a header value where the host is to
/// put the secret the person stored for that slot.
#[must_use]
pub fn secret_marker(slot: &str) -> String {
    format!("{SECRET_MARKER_OPEN}{slot}}}}}")
}

/// How rCommander connects to a host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Protocol {
    Ssh,
    Sftp,
    Ftp,
    Ftps,
}

impl Protocol {
    /// The name the WIT enum uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Sftp => "sftp",
            Self::Ftp => "ftp",
            Self::Ftps => "ftps",
        }
    }
}

/// One host of the inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryHost {
    /// The inventory's own id of the host, stable across renames: what a connection is matched
    /// by.
    pub external_id: String,
    pub name: String,
    /// A host name or an IP address, nothing else: no scheme, no user, no path.
    pub address: String,
    pub port: u16,
    pub protocol: Protocol,
    pub user: Option<String>,
    /// The folder below the source's target folder, `/`-separated; empty for the target itself.
    pub folder: String,
    pub tags: Vec<String>,
    pub note: Option<String>,
}

/// What an inventory may ask of its host: the `host`, `settings` and `http` interfaces of its
/// world.
pub trait InventoryHostApi {
    /// Writes a line to rCommander's log; the host caps and masks it.
    fn log(&mut self, level: &str, message: &str);
    /// The value of the manifest's setting `name`; `None` for a name it does not declare.
    fn setting(&mut self, name: &str) -> Option<String>;
    /// One request to the address the person entered for the source; a redirect is answered,
    /// never followed. A header value may carry [`secret_marker`]s.
    fn http(
        &mut self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<HttpResponse, Failure>;
}

/// The logic of an inventory: `interface inventory`. A store lives for one call.
pub trait Inventory {
    /// Every host of the inventory, at most [`MAX_HOSTS`].
    fn list(&mut self, host: &mut dyn InventoryHostApi) -> Result<Vec<InventoryHost>, Failure>;
}

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
