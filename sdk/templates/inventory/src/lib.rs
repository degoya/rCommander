//! {{PLUGIN_NAME}}: an inventory. rCommander asks it for the hosts of a server inventory and
//! keeps a folder of connections in step with them; the person's own fields of a connection
//! (credentials, login script) are never touched.
//!
//! It asks the address the person entered for the source with one plain-text request - an
//! example to replace with your inventory's own API (NetBox's `/api/dcim/devices/`, Proxmox's
//! `/api2/json/cluster/resources`):
//!
//! - `GET /hosts.tsv`, with `Authorization: Token {{secret:api_token}}`: one host per line, its
//!   id, name, address, port, protocol, user, folder and tags separated by tabs (tags by commas).
//!
//! The path is relative: rCommander resolves it against the address the person entered, so the
//! plugin never needs to know where its server is. The token is a marker rCommander fills in on
//! the way out; the plugin never holds it. The logic is plain Rust over the traits of
//! `types.rs`; `src/guest.rs` translates it into the contract.

pub mod types;

#[cfg(target_arch = "wasm32")]
mod guest;

use types::{Failure, FailureKind, Http, InventoryHost, Protocol};

/// The one request of the example.
pub const HOSTS: &str = "/hosts.tsv";
/// The header the token goes in; the marker names the slot of `manifest.toml`.
pub const AUTHORIZATION: (&str, &str) = ("Authorization", "Token {{secret:api_token}}");

/// Every host of the inventory. rCommander checks each one again and leaves out what breaks its
/// rules; a line this plugin cannot read is left out here already.
pub fn list(http: &mut impl Http) -> Result<Vec<InventoryHost>, Failure> {
    let headers = [(AUTHORIZATION.0.to_owned(), AUTHORIZATION.1.to_owned())];
    let answer = http.get(HOSTS, &headers)?;
    let body = match answer.status {
        200 => String::from_utf8(answer.body).map_err(|_| Failure::of(FailureKind::BadData))?,
        401 | 403 => return Err(Failure::of(FailureKind::AccessDenied)),
        _ => return Err(Failure::of(FailureKind::Io)),
    };
    Ok(body.lines().filter_map(host).collect())
}

/// One line of `/hosts.tsv`, or `None` when it is not one.
fn host(line: &str) -> Option<InventoryHost> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [id, name, address, port, protocol, user, folder, tags] = fields.as_slice() else {
        return None;
    };
    let protocol = match *protocol {
        "ssh" => Protocol::Ssh,
        "sftp" => Protocol::Sftp,
        "ftp" => Protocol::Ftp,
        "ftps" => Protocol::Ftps,
        _ => return None,
    };
    Some(InventoryHost {
        external_id: (*id).to_owned(),
        name: (*name).to_owned(),
        address: (*address).to_owned(),
        port: port.parse().ok()?,
        protocol,
        user: (!user.is_empty()).then(|| (*user).to_owned()),
        folder: (*folder).to_owned(),
        tags: tags
            .split(',')
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect(),
        note: None,
    })
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
