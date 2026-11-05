//! Drawing the MCP page's call log (`mcp_log.rs`): the filter by session, Refresh and Clear,
//! then one line per call - when, session, tool, class, decision, the answer's code and the
//! arguments as they were logged, masked and cut.

use iced::{
    Alignment, Element,
    widget::{Column, column, pick_list, row},
};
use rc_db::McpAuditEntry;

use super::{
    host_keys_view::shown_time,
    mcp_log::{McpLogMsg, McpLogPage, SessionChoice, page_message},
    view::{hint, label},
};
use crate::{
    app::Message,
    i18n::t,
    theme::{Tokens, focus, layout},
};

/// How many characters of a call's arguments its line shows.
const ARGUMENTS_SHOWN: usize = 160;

/// The decision as the page names it.
pub fn decision_title(decision: &str) -> &'static str {
    match decision {
        "allowed" => t!("settings.mcp.log.decision.allowed"),
        "asked" => t!("settings.mcp.log.decision.asked"),
        "confirmed" => t!("settings.mcp.log.decision.confirmed"),
        "declined" => t!("settings.mcp.log.decision.declined"),
        "denied" => t!("settings.mcp.log.decision.denied"),
        "refused" => t!("settings.mcp.log.decision.refused"),
        _ => t!("settings.mcp.log.decision.unknown"),
    }
}

/// `text` cut to `max` characters, with `...` where it was cut.
fn clipped(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}...", &text[..at]),
        None => text.to_owned(),
    }
}

/// One call's line.
pub fn entry_line(entry: &McpAuditEntry) -> String {
    format!(
        "{}  {}  {}  {}  {}  {}  {}",
        shown_time(&entry.recorded_at),
        entry.session,
        entry.tool,
        entry.class.as_deref().unwrap_or("-"),
        decision_title(&entry.decision),
        entry.code,
        clipped(&entry.arguments, ARGUMENTS_SHOWN)
    )
}

pub fn view<'a>(page: &'a McpLogPage, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut options = vec![SessionChoice(None)];
    options.extend(page.sessions.iter().cloned().map(Some).map(SessionChoice));
    let chosen = SessionChoice(page.session.clone());
    let filter = |choice: SessionChoice| page_message(McpLogMsg::Filter(choice));
    // The filter is one stop; the arrows choose the session (RC-080-16).
    let moves = focus::moves(&options, Some(&chosen), &filter);
    let list = pick_list(options, Some(chosen), filter).width(280);
    let ready = |message: McpLogMsg| (!page.busy).then(|| page_message(message));
    let controls = row![
        label(t!("settings.mcp.log.session")),
        focus::Stop::pick(list, "settings.mcp.log.session", moves),
        layout::secondary_button(t!("settings.mcp.log.refresh"), ready(McpLogMsg::Refresh)),
        layout::secondary_button(t!("settings.mcp.log.clear"), ready(McpLogMsg::Clear)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut lines = Column::new().spacing(4);
    match page.entries.as_deref() {
        None => lines = lines.push(hint(t!("settings.loading"), tokens)),
        Some([]) => lines = lines.push(hint(t!("settings.mcp.log.empty"), tokens)),
        Some(entries) => {
            for entry in entries {
                lines = lines.push(hint(entry_line(entry), tokens));
            }
        }
    }
    column![
        label(t!("settings.mcp.log")),
        hint(t!("settings.mcp.log.help"), tokens),
        controls,
        lines,
    ]
    .spacing(8)
    .into()
}
