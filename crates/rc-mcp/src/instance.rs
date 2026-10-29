//! The instance file: how `rcommander mcp` finds a running rCommander to forward to.
//!
//! The HTTP server writes `<data dir>/mcp-instance.json` (`{port, token, pid}`, mode 0600 on
//! Unix; on Windows the data directory under `%APPDATA%` is the user's own) and removes it when
//! it stops. A file left behind by a crash is harmless: [`probe`] asks the port before anything
//! is forwarded, and only an rCommander that knows the token answers. The probe never sends the
//! token: it sends a fresh nonce, and the server proves the token with a keyed hash over its port
//! and the nonce - whoever holds the port after a crash learns nothing (RC-080-12).

use std::{
    fmt, fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
    time::Duration,
};

use rc_db::Db;
use serde::{Deserialize, Serialize};

use crate::{McpToken, confirm::random_hex};

/// The file name inside the data directory.
pub const INSTANCE_FILE: &str = "mcp-instance.json";

/// The most [`read`] takes of the file: a port, a 64-digit token and a pid fit many times over;
/// anything larger is no instance file of ours and is not read into memory.
const MAX_INSTANCE_BYTES: u64 = 4 * 1024;

/// How long [`probe`] waits for an answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// What a running instance tells `rcommander mcp`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    /// The port on 127.0.0.1.
    pub port: u16,
    /// The bearer token.
    pub token: String,
    /// The process that wrote the file.
    pub pid: u32,
}

impl fmt::Debug for Instance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Instance")
            .field("port", &self.port)
            .field("token", &"[redacted]")
            .field("pid", &self.pid)
            .finish()
    }
}

impl Instance {
    /// The MCP endpoint.
    #[must_use]
    pub fn mcp_url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port)
    }
}

/// `mcp-instance.json` next to `rcommander.db`.
///
/// # Errors
///
/// The platform reports no data directory.
pub fn default_path() -> rc_db::Result<PathBuf> {
    let db = Db::default_path()?;
    Ok(db.parent().map_or_else(
        || PathBuf::from(INSTANCE_FILE),
        |dir| dir.join(INSTANCE_FILE),
    ))
}

/// Writes the file as rCommander writes every private file (`rc_db::write_private_file`,
/// RC-080-42): under a temporary name made with `create_new` beside it, owner-only on Unix,
/// flushed, then renamed over the old one - so a reader never sees half a file and a link
/// planted at the temporary name is never followed.
///
/// # Errors
///
/// The directory or the file cannot be written.
pub fn write(path: &Path, instance: &Instance) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_vec_pretty(instance).map_err(io::Error::other)?;
    rc_db::write_private_file(path, &text, true).map_err(io::Error::other)
}

/// The instance the file names, if it reads; `None` for a file over [`MAX_INSTANCE_BYTES`].
#[must_use]
pub fn read(path: &Path) -> Option<Instance> {
    let mut text = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_INSTANCE_BYTES + 1)
        .read_to_end(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_INSTANCE_BYTES {
        return None;
    }
    serde_json::from_slice(&text).ok()
}

/// Removes the file if it still names `port` of this process: another instance may have
/// replaced it since.
pub fn remove_own(path: &Path, port: u16) {
    if read(path)
        .is_some_and(|instance| instance.port == port && instance.pid == std::process::id())
    {
        let _ = fs::remove_file(path);
    }
}

/// Whether an rCommander that knows the instance's token answers on its port: the answer to a
/// fresh nonce must be the token's proof for this port and nonce. The token itself is not sent.
pub async fn probe(instance: &Instance) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    let nonce = random_hex(32);
    let expected = McpToken::from_text(instance.token.clone()).proof(instance.port, &nonce);
    let answer = client
        .get(format!(
            "http://127.0.0.1:{}/health?nonce={nonce}",
            instance.port
        ))
        .send()
        .await;
    match answer {
        Ok(response) if response.status().is_success() => response
            .json::<serde_json::Value>()
            .await
            .is_ok_and(|body| {
                body["app"] == "rcommander"
                    && body["proof"]
                        .as_str()
                        .and_then(|proof| blake3::Hash::from_hex(proof).ok())
                        .is_some_and(|proof| proof == expected)
            }),
        _ => false,
    }
}

#[cfg(test)]
#[path = "instance_tests.rs"]
mod tests;
