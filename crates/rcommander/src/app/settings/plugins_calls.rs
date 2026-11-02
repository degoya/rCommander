//! The selected plugin's calls on the Plugins page (RC-100-75): one line that sums up every
//! retained call - their number, the failures with the newest one's code, the mean duration -
//! above the latest calls, which open on a click (a stop of the focus ring) and are closed by
//! default. `plugins.recent_calls_shown` sets how many the engine answers with; at 0 the list
//! and its toggle are left out, the summary stays.

use iced::widget::{Column, button, column, row};
use rc_db::{PluginExecution, PluginExecutionSummary};

use super::{
    plugins::{PluginsMsg, PluginsPage, page_message},
    view::{hint, label},
};
use crate::{
    app::Message,
    i18n::t,
    theme::{Tokens, fit, focus},
};

/// What the list of latest calls shows.
#[derive(Debug, PartialEq, Eq)]
pub enum Shown<'a> {
    /// No list: `plugins.recent_calls_shown` is 0, or no call is recorded.
    Hidden,
    /// The toggle alone.
    Closed,
    /// The toggle and the calls, newest first.
    Open(&'a [PluginExecution]),
}

/// What the list of `page`'s latest calls shows.
pub fn shown(page: &PluginsPage) -> Shown<'_> {
    let calls = &page.calls;
    if calls.shown == 0 || calls.latest.is_empty() {
        Shown::Hidden
    } else if page.calls_open {
        Shown::Open(&calls.latest)
    } else {
        Shown::Closed
    }
}

/// The summary's line: how many calls, how many failed and the newest failure's code, the mean
/// duration; `None` without a call.
pub fn summary_line(summary: &PluginExecutionSummary) -> Option<String> {
    if summary.calls == 0 {
        return None;
    }
    Some(match &summary.last_failure {
        Some(code) if summary.failures > 0 => t!(
            "settings.plugins.calls_summary_failed",
            calls = summary.calls,
            failures = summary.failures,
            code = code,
            mean = summary.mean_duration_ms
        ),
        _ => t!(
            "settings.plugins.calls_summary",
            calls = summary.calls,
            failures = summary.failures,
            mean = summary.mean_duration_ms
        ),
    })
}

/// One call: when, version, export, outcome and code, duration.
fn call_line(call: &PluginExecution) -> String {
    format!(
        "{}  {} {}  {} {}  {} ms",
        super::host_keys_view::shown_time(&call.recorded_at),
        call.plugin_version,
        call.operation,
        call.outcome,
        call.code.as_deref().unwrap_or_default(),
        call.duration_ms
    )
}

/// The heading, the summary, and the latest calls behind their toggle.
pub fn section<'a>(page: &'a PluginsPage, tokens: &'static Tokens) -> Column<'a, Message> {
    let summary = page
        .calls
        .summary
        .as_ref()
        .and_then(summary_line)
        .unwrap_or_else(|| t!("settings.plugins.log_empty").to_owned());
    let mut section =
        column![label(t!("settings.plugins.calls")), hint(summary, tokens)].spacing(6);
    let open = match shown(page) {
        Shown::Hidden => return section,
        Shown::Closed => None,
        Shown::Open(calls) => Some(calls),
    };
    let marker = if open.is_some() {
        "\u{25be}"
    } else {
        "\u{25b8}"
    };
    let toggle = page_message(PluginsMsg::ToggleCalls);
    let face = row![
        fit::line(marker).color(tokens.accent),
        label(t!("settings.plugins.log")),
    ]
    .spacing(6);
    let control = button(face)
        .padding(0)
        .style(button::text)
        .on_press(toggle.clone());
    // One label in both states, so the ring stays on the toggle as it opens and closes.
    section = section.push(focus::button(control, "settings.plugins.log", Some(toggle)));
    for call in open.into_iter().flatten() {
        section = section.push(hint(call_line(call), tokens));
    }
    section
}

#[cfg(test)]
#[path = "plugins_calls_tests.rs"]
mod tests;
