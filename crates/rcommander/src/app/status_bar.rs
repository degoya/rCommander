//! The status bar at the window's bottom (under the function-key bar of `file_ops/fkey_bar.rs`),
//! which tells whether the MCP server runs and where, that an update is there (RC-100-05), and
//! how many port forwards run (RC-110-03, `ssh.forwards_in_status_bar`) and how many server
//! files are being edited through copies (RC-110-01), a click away from their list.
//! The ways into the connection manager, the database dialog and the settings are in the menu
//! bar (RC-080-18).

use iced::{
    Element, Length,
    widget::{Space, row, text},
};

use super::{App, Message};
use crate::{
    i18n::t,
    mcp::McpStatus,
    theme::{
        self, Tokens,
        fit::{self, label_or_glyph, line},
    },
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
        forwards(app, tokens),
        super::quit::remote_edit::status(app, tokens),
        line(mcp)
            .size(theme::STATUS_SIZE)
            .color(tokens.text_secondary),
    ]
    .spacing(16)
    .padding([0, 4])
    .into()
}

/// "Forwards: 2" while port forwards run, each in the tooltip as `127.0.0.1:5432 -> db:5432
/// (name)`; nothing while none runs or the setting hides it. Read from the engine each frame: a
/// lock and the few running ones.
fn forwards<'a>(app: &App, tokens: &'static Tokens) -> Element<'a, Message> {
    let running = app.engine.status_bar_forwards();
    if running.is_empty() {
        return Space::new().into();
    }
    let lines: Vec<String> = running
        .iter()
        .map(|forward| match &forward.name {
            Some(name) => format!("{} ({name})", forward.label()),
            None => forward.label(),
        })
        .collect();
    let face = label_or_glyph(t!("status.forwards", count = running.len()))
        .size(theme::STATUS_SIZE)
        .color(tokens.text_secondary)
        .wrapping(text::Wrapping::None);
    fit::tip(face, lines.join(", "), tokens)
}
