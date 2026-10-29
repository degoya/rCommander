//! The MCP server's rows, all protected: an agent never loosens its own permissions. Split from
//! `schema.rs` to keep it under 500 lines (RC-100-09).

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether a headless `rcommander mcp`, with no window to ask in, lets a confirmation code stand
/// in for the person's answer (RC-010-13). Off by default: headless, "ask" is "deny".
pub const MCP_HEADLESS_CODE_KEY: &str = "mcp.ask.headless_code";

/// How many minutes "allow for this session" holds for an executing tool on the terminal or
/// connection its call names (RC-010-27); 0 never offers it there.
pub const MCP_EXEC_SESSION_MINUTES_KEY: &str = "mcp.ask.exec_session_minutes";

pub(super) const HTTP_ENABLED: SettingSpec = SettingSpec {
    key: "mcp.http.enabled",
    page: SettingPage::Mcp,
    kind: SettingKind::Bool { default: true },
    label: "setting.mcp.http.enabled",
    help: "setting.mcp.http.enabled.help",
    protected: true,
    restart: false,
};

pub(super) const HTTP_PORT: SettingSpec = SettingSpec {
    key: "mcp.http.port",
    page: SettingPage::Mcp,
    kind: SettingKind::Int {
        min: 0,
        max: 65535,
        default: 0,
    },
    label: "setting.mcp.http.port",
    help: "setting.mcp.http.port.help",
    protected: true,
    restart: false,
};

pub(super) const HEADLESS_CODE: SettingSpec = SettingSpec {
    key: MCP_HEADLESS_CODE_KEY,
    page: SettingPage::Mcp,
    kind: SettingKind::Bool { default: false },
    label: "setting.mcp.ask.headless_code",
    help: "setting.mcp.ask.headless_code.help",
    protected: true,
    restart: false,
};

pub(super) const EXEC_SESSION_MINUTES: SettingSpec = SettingSpec {
    key: MCP_EXEC_SESSION_MINUTES_KEY,
    page: SettingPage::Mcp,
    kind: SettingKind::Int {
        min: 0,
        max: 480,
        default: 30,
    },
    label: "setting.mcp.ask.exec_session_minutes",
    help: "setting.mcp.ask.exec_session_minutes.help",
    protected: true,
    restart: false,
};
