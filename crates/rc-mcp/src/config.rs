//! The server's switch and port: the settings `mcp.http.enabled` (default on) and
//! `mcp.http.port` (0, the default, picks a free port), read through the engine's schema - a bad
//! stored value counts as the default and is logged.
//!
//! The one switch covers both transports: switched off, the app starts no HTTP server and
//! `rcommander mcp` (stdio) refuses with [`DISABLED_MESSAGE`]. Like the token and the policy,
//! both are protected settings: only the settings dialog changes them, never `settings_set`.

use rc_db::Db;
use rc_engine::settings::stored_setting;

/// The setting that switches the server on (`true`, the default) or off.
pub const ENABLED_KEY: &str = "mcp.http.enabled";

/// The setting that fixes the port; 0 picks a free one.
pub const PORT_KEY: &str = "mcp.http.port";

/// What `rcommander mcp` says when the server is switched off.
pub const DISABLED_MESSAGE: &str = "rCommander's MCP server is switched off; a person can switch \
it on in rCommander under Settings > MCP";

/// Whether the server may run.
pub async fn enabled(db: &Db) -> bool {
    stored_setting(db, ENABLED_KEY)
        .await
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

/// The port to bind on 127.0.0.1; 0 for any free one.
pub async fn port(db: &Db) -> u16 {
    stored_setting(db, PORT_KEY)
        .await
        .and_then(|value| value.as_u64())
        .and_then(|port| u16::try_from(port).ok())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
