//! The status bar at the window's bottom (under the function-key bar of `file_ops/fkey_bar.rs`),
//! which tells whether the MCP server runs and where, and that an update is there (RC-100-05).
//! The ways into the connection manager, the database dialog and the settings are in the menu
//! bar (RC-080-18).

use iced::{Element, Length, widget::row};

use super::{App, Message};
use crate::{
    i18n::t,
    mcp::McpStatus,
    theme::{self, Tokens, fit::line},
};

/// One quiet line: a start-up notice (or else the latest news) on the left, the waiting MCP
/// questions, the calls allowed for a session and the MCP server on the right.
pub(super) fn status_bar<'a>(app: &'a App, tokens: &'static Tokens) -> Element<'a, Message> {
    let mcp = match &app.mcp {
        McpStatus::Off => t!("status.mcp_off").to_owned(),
        McpStatus::Serving(address) => t!("status.mcp_on", address = address),
        McpStatus::Failed(reason) => t!("status.mcp_failed", error = reason),
    };
    let (left, color) = match (&app.notice, &app.info) {
        (Some(notice), _) => (notice.as_str(), theme::iced_theme(tokens).palette().danger),
        (None, Some(info)) => (info.as_str(), tokens.text_secondary),
        (None, None) => ("", tokens.text_secondary),
    };
    row![
        line(left)
            .size(theme::STATUS_SIZE)
            .color(color)
            .width(Length::Fill),
        super::update_ui::notice(app, tokens),
        super::mcp_confirm::status(&app.mcp_questions, tokens),
        super::mcp_confirm::grants(&app.mcp_grants, tokens),
        line(mcp)
            .size(theme::STATUS_SIZE)
            .color(tokens.text_secondary),
    ]
    .spacing(16)
    .padding([0, 4])
    .into()
}
