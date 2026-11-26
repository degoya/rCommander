//! The argument enums of the `connections` tools, as tools name a connection's protocol, its
//! authentication and its view; apart from `connections.rs` to keep it under 500 lines.

use rc_engine::{AuthMethod, ConnectionKind, DefaultView};
use rmcp::schemars;
use serde::Deserialize;

/// A connection's protocol, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KindArg {
    Ssh,
    Ftp,
    Ftps,
    FtpsImplicit,
}

impl From<KindArg> for ConnectionKind {
    fn from(kind: KindArg) -> Self {
        match kind {
            KindArg::Ssh => Self::Ssh,
            KindArg::Ftp => Self::Ftp,
            KindArg::Ftps => Self::Ftps,
            KindArg::FtpsImplicit => Self::FtpsImplicit,
        }
    }
}

/// How a connection authenticates, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthArg {
    Password,
    Key,
    Agent,
    Interactive,
    Anonymous,
}

impl From<AuthArg> for AuthMethod {
    fn from(method: AuthArg) -> Self {
        match method {
            AuthArg::Password => Self::Password,
            AuthArg::Key => Self::Key,
            AuthArg::Agent => Self::Agent,
            AuthArg::Interactive => Self::Interactive,
            AuthArg::Anonymous => Self::Anonymous,
        }
    }
}

/// What a panel shows when the connection opens, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ViewArg {
    Files,
    Terminal,
}

impl From<ViewArg> for DefaultView {
    fn from(view: ViewArg) -> Self {
        match view {
            ViewArg::Files => Self::Files,
            ViewArg::Terminal => Self::Terminal,
        }
    }
}
