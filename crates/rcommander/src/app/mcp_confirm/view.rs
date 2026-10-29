//! Drawing the question: a medium card over the dimmed window or a window of its own
//! (RC-080-23, `dialog_windows/questions.rs`), and in the status bar the count of the questions
//! and of the calls allowed for a session (RC-010-27).

use iced::{
    Element, Length,
    widget::{Column, Space, button, column, container, row, scrollable, text},
};
use rc_engine::{CommandClass, McpAnswer, McpGrant, McpQuestion, McpSessionOffer, PanelId};

use super::McpConfirmMsg;
use crate::{
    app::Message,
    i18n::t,
    theme::{self, Tokens, fit::label_or_glyph, focus, icon::Icon, layout},
};

const DETAILS_HEIGHT: f32 = 320.0;

/// The id of the Deny button, which has the focus ring as a question opens.
pub const DENY_ID: &str = "rcommander.mcp_confirm.deny";

/// The oldest open question over the window; `None` while none waits.
pub fn layer<'a>(
    questions: &'a [McpQuestion],
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let title = label_or_glyph(title(questions)?).font(theme::UI_FONT_STRONG);
    Some(layout::dialog_card(
        Message::dialog_head(title),
        body(questions, tokens)?,
        layout::MEDIUM_DIALOG_WIDTH,
        tokens,
    ))
}

/// The oldest open question's title: in its card's head, or in its window's title bar.
pub fn title(questions: &[McpQuestion]) -> Option<String> {
    let question = questions.first()?;
    Some(t!("mcp_confirm.title", tool = question.tool))
}

/// The oldest open question - the call, its arguments, where it acts - and its buttons: on its
/// card, or filling a window of its own.
pub fn body<'a>(
    questions: &'a [McpQuestion],
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let question = questions.first()?;
    let mut details = Column::new()
        .spacing(6)
        .push(label_or_glyph(t!("mcp_confirm.arguments")).color(tokens.text_secondary));
    if question.arguments.is_empty() {
        details = details.push(label_or_glyph(t!("mcp_confirm.no_arguments")));
    }
    for argument in &question.arguments {
        details = details.push(
            row![
                label_or_glyph(argument.name.as_str())
                    .color(tokens.text_secondary)
                    .width(120),
                text(argument.value.as_str())
                    .size(theme::TEXT_SIZE)
                    .font(theme::MONO_FONT)
                    .wrapping(text::Wrapping::Glyph)
                    .width(Length::Fill),
            ]
            .spacing(8),
        );
    }
    details = details.push(label_or_glyph(panel_line(question)).color(tokens.text_secondary));
    let mut body = column![
        label_or_glyph(t!("mcp_confirm.class", class = class_title(question.class))),
        container(scrollable(details)).max_height(DETAILS_HEIGHT),
        label_or_glyph(t!("mcp_confirm.note")).color(tokens.text_secondary),
    ]
    .spacing(10);
    if questions.len() > 1 {
        body = body.push(label_or_glyph(t!(
            "mcp_confirm.more_waiting",
            count = questions.len() - 1
        )));
    }
    let id = question.id;
    let answer = |answer: McpAnswer| Some(Message::McpConfirm(McpConfirmMsg::Answer(id, answer)));
    let mut secondary = Vec::new();
    // Only a click allows (RC-010-13): the Allow buttons are no stops of the focus ring, so no
    // key typed as the question appears - Tab, then Enter - can allow a call.
    if let Some(offer) = question.session {
        // An executing tool's grant is bound to the terminal or connection its call names and
        // runs out (RC-010-27); the button says so.
        let caption = match offer {
            McpSessionOffer::Session => t!("mcp_confirm.allow_session").to_owned(),
            McpSessionOffer::Minutes(minutes) => t!("mcp_confirm.allow_here", minutes = minutes),
        };
        secondary.push(click_only(
            None,
            caption,
            answer(McpAnswer::AllowForSession),
            button::secondary,
        ));
    }
    // Deny has the ring as the question opens (`DENY_ID`): Enter denies, as the safe answer of
    // a Windows prompt is its default focus (RC-080-16).
    let deny = answer(McpAnswer::Deny);
    let face = button(layout::icon_button(Icon::Close, t!("mcp_confirm.deny")))
        .padding(layout::BUTTON_PADDING)
        .style(button::secondary)
        .on_press_maybe(deny.clone());
    secondary.push(
        focus::Stop::button(face, t!("mcp_confirm.deny"), deny)
            .id(DENY_ID)
            .into(),
    );
    let buttons = layout::button_row(
        Some(click_only(
            Some(Icon::Confirm),
            t!("mcp_confirm.allow"),
            answer(McpAnswer::Allow),
            button::primary,
        )),
        secondary,
        [],
    );
    Some(body.push(buttons).into())
}

/// A dialog button that only a click presses: no stop of the focus ring.
fn click_only<'a>(
    icon: Option<Icon>,
    label: impl text::IntoFragment<'a>,
    on_press: Option<Message>,
    style: fn(&iced::Theme, button::Status) -> button::Style,
) -> Element<'a, Message> {
    button(layout::icon_button(icon, label))
        .padding(layout::BUTTON_PADDING)
        .style(style)
        .on_press_maybe(on_press)
        .into()
}

/// The status bar's count of waiting questions; nothing while none waits.
pub fn status<'a>(questions: &[McpQuestion], tokens: &'static Tokens) -> Element<'a, Message> {
    if questions.is_empty() {
        return Space::new().into();
    }
    label_or_glyph(t!("status.mcp_waiting", count = questions.len()))
        .size(theme::STATUS_SIZE)
        .color(tokens.accent)
        .wrapping(text::Wrapping::None)
        .into()
}

/// The status bar's count of the calls allowed for a session, which a click takes back
/// (`mcp.revoke`, RC-010-27); nothing while none holds.
pub fn grants<'a>(grants: &[McpGrant], tokens: &'static Tokens) -> Element<'a, Message> {
    if grants.is_empty() {
        return Space::new().into();
    }
    let face = label_or_glyph(t!("status.mcp_allowed", count = grants.len()))
        .size(theme::STATUS_SIZE)
        .color(tokens.accent)
        .wrapping(text::Wrapping::None);
    let revoke = button(face)
        .padding(0)
        .style(button::text)
        .on_press(Message::McpConfirm(McpConfirmMsg::Revoke));
    theme::fit::tip(revoke, t!("status.mcp_revoke").to_owned(), tokens)
}

/// Where the call acts: the panel's folder, and its selection or the entry under its cursor.
fn panel_line(question: &McpQuestion) -> String {
    let context = &question.panel;
    let folder = match context.panel {
        PanelId::Left => t!("mcp_confirm.panel_left", path = context.path),
        PanelId::Right => t!("mcp_confirm.panel_right", path = context.path),
    };
    let names = context.entries.join(", ");
    let entries = match (context.selection, context.entries.is_empty()) {
        (_, true) => return folder,
        (true, false) if context.more > 0 => {
            t!(
                "mcp_confirm.selected_more",
                names = names,
                more = context.more
            )
        }
        (true, false) => t!("mcp_confirm.selected", names = names),
        (false, false) => t!("mcp_confirm.cursor", name = names),
    };
    format!("{folder} {entries}")
}

fn class_title(class: CommandClass) -> &'static str {
    match class {
        CommandClass::Read => t!("settings.mcp.class.read"),
        CommandClass::Write => t!("settings.mcp.class.write"),
        CommandClass::Destructive => t!("settings.mcp.class.destructive"),
        CommandClass::Exec => t!("settings.mcp.class.exec"),
        CommandClass::GuiOnly => t!("settings.mcp.class.gui_only"),
    }
}
