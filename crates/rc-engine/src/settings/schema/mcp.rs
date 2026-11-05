//! The MCP server's rows, all protected: an agent never loosens its own permissions. Split from
//! `schema.rs` to keep it under 500 lines (RC-100-09).

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether a headless `rcommander mcp`, with no window to ask in, lets a confirmation code stand
/// in for the person's answer (RC-010-13). Off by default: headless, "ask" is "deny".
pub const MCP_HEADLESS_CODE_KEY: &str = "mcp.ask.headless_code";

/// How many minutes "allow for this session" holds for an executing tool on the terminal or
/// connection its call names (RC-010-27); 0 never offers it there.
pub const MCP_EXEC_SESSION_MINUTES_KEY: &str = "mcp.ask.exec_session_minutes";

/// Whether a reading or writing tool asks first when a path it names lies outside the folders
/// the two panels show (the audit's H2). Off by default: an agent reads and writes where the
/// policy allows, the deny lists aside.
pub const MCP_ASK_OUTSIDE_PANELS_KEY: &str = "mcp.ask.outside_panels";

pub(super) const ASK_OUTSIDE_PANELS: SettingSpec = SettingSpec {
    key: MCP_ASK_OUTSIDE_PANELS_KEY,
    page: SettingPage::Mcp,
    kind: SettingKind::Bool { default: false },
    label: "setting.mcp.ask.outside_panels",
    help: "setting.mcp.ask.outside_panels.help",
    protected: true,
    restart: false,
};

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

/// Whether every app start gives the HTTP server a new bearer token (RC-080-66). Off by default
/// (owner 2026-11-05): an HTTP client configured with the token would break at each start.
pub const MCP_TOKEN_ROTATE_ON_START_KEY: &str = "mcp.token.rotate_on_start";

pub(super) const TOKEN_ROTATE_ON_START: SettingSpec = SettingSpec {
    key: MCP_TOKEN_ROTATE_ON_START_KEY,
    page: SettingPage::Mcp,
    kind: SettingKind::Bool { default: false },
    label: "setting.mcp.token.rotate_on_start",
    help: "setting.mcp.token.rotate_on_start.help",
    protected: true,
    restart: true,
};

/// Whether the gate writes a row per tool call into the MCP audit log (`mcp_audit_log`,
/// RC-080-66). On by default.
pub const MCP_AUDIT_LOG_KEY: &str = "mcp.audit_log";

pub(super) const AUDIT_LOG: SettingSpec = SettingSpec {
    key: MCP_AUDIT_LOG_KEY,
    page: SettingPage::Mcp,
    kind: SettingKind::Bool { default: true },
    label: "setting.mcp.audit_log",
    help: "setting.mcp.audit_log.help",
    protected: true,
    restart: false,
};
