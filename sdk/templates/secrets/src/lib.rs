//! {{PLUGIN_NAME}}: a secret source. rCommander asks it for a connection's password while it
//! connects; the source asks its password manager and answers. It never receives a secret of
//! rCommander's.
//!
//! It speaks to a local bridge on `localhost:8087`, the one port `manifest.toml` grants, with
//! three plain-text requests - an example to replace with your manager's own API:
//!
//! - `GET /status`: `ready`, or `locked` while the manager waits to be unlocked;
//! - `GET /search?q=<query>`: one entry per line, its id, title, user and address separated by
//!   tabs;
//! - `GET /secret/<id>/<field>`: the value.
//!
//! The logic is plain Rust over the traits of `types.rs`; `src/guest.rs` translates it into the
//! contract.

pub mod types;

#[cfg(target_arch = "wasm32")]
mod guest;

use types::{Failure, FailureKind, Http, SecretEntry, SecretField, SourceState};

/// The bridge the manifest grants.
pub const BRIDGE: &str = "http://localhost:8087";

/// Whether the manager answers. A request that fails means the bridge is not running.
pub fn status(http: &mut impl Http) -> SourceState {
    match http.get(&format!("{BRIDGE}/status")) {
        Ok(answer) if answer.status == 200 && answer.body.trim_ascii() == b"ready" => {
            SourceState::Ready
        }
        Ok(answer) if answer.status == 200 && answer.body.trim_ascii() == b"locked" => {
            SourceState::Locked
        }
        _ => SourceState::Unreachable,
    }
}

/// At most `limit` entries matching `query`. Only ids, titles, users and addresses: a value never
/// goes into a search answer.
pub fn search(http: &mut impl Http, query: &str, limit: u32) -> Result<Vec<SecretEntry>, Failure> {
    let body = text(http.get(&format!("{BRIDGE}/search?q={}", encode(query)))?)?;
    Ok(body
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next().filter(|id| !id.is_empty())?;
            let title = fields.next().unwrap_or(id);
            let optional =
                |field: Option<&str>| field.filter(|text| !text.is_empty()).map(str::to_owned);
            Some(SecretEntry {
                id: id.to_owned(),
                title: title.to_owned(),
                user: optional(fields.next()),
                url: optional(fields.next()),
            })
        })
        .take(limit as usize)
        .collect())
}

/// The secret `field` of entry `id`.
pub fn get(http: &mut impl Http, id: &str, field: SecretField) -> Result<String, Failure> {
    let value = text(http.get(&format!(
        "{BRIDGE}/secret/{}/{}",
        encode(id),
        field.as_str()
    ))?)?;
    if value.is_empty() {
        return Err(Failure::of(FailureKind::NotFound));
    }
    Ok(value)
}

/// The body of a `200` as text; `404` is an entry the manager does not have.
fn text(answer: types::HttpResponse) -> Result<String, Failure> {
    match answer.status {
        200 => String::from_utf8(answer.body).map_err(|_| Failure::of(FailureKind::BadData)),
        404 => Err(Failure::of(FailureKind::NotFound)),
        _ => Err(Failure::of(FailureKind::Io)),
    }
}

/// `text` as one path segment or query value: unreserved characters as they are, every other
/// byte percent-encoded.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
