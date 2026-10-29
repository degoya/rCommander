//! The MCP page below its schema rows: the rule of each tool class and the access token.
//!
//! The token is never shown: it is copied to the clipboard or replaced, and a new one is in
//! force at once (the server restarts with it). Nothing on this page can be changed over MCP.

use iced::{
    Alignment, Element,
    widget::{Column, Row, button, row},
};
use rc_engine::CommandClass;
use rc_mcp::Rule;

use super::{
    SettingsMessage,
    draft::{Draft, POLICY_CLASSES, rule_of, rules_for},
    view::{hint, label},
    wrap,
};
use crate::{
    app::Message,
    i18n::t,
    theme::{Tokens, focus, icon::Icon, layout},
};

fn class_title(class: CommandClass) -> &'static str {
    match class {
        CommandClass::Read => t!("settings.mcp.class.read"),
        CommandClass::Write => t!("settings.mcp.class.write"),
        CommandClass::Destructive => t!("settings.mcp.class.destructive"),
        CommandClass::Exec => t!("settings.mcp.class.exec"),
        CommandClass::GuiOnly => t!("settings.mcp.class.gui_only"),
    }
}

fn rule_title(rule: Rule) -> &'static str {
    match rule {
        Rule::Allowed => t!("settings.mcp.rule.allowed"),
        Rule::Ask => t!("settings.mcp.rule.ask"),
        Rule::Deny => t!("settings.mcp.rule.deny"),
    }
}

/// A button of the token, with the access key its label marks.
fn token_button<'a>(
    icon: Option<Icon>,
    title: &'static str,
    message: SettingsMessage,
) -> Element<'a, Message> {
    let face = button(layout::marked_icon_button(icon, title))
        .padding([6, 12])
        .style(button::secondary)
        .on_press(wrap(message.clone()));
    layout::keyed(focus::Stop::button(face, title, Some(wrap(message))), title)
}

pub(super) fn view<'a>(draft: &'a Draft, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut rules = Column::new()
        .spacing(8)
        .push(label(t!("settings.mcp.rights")))
        .push(hint(t!("settings.mcp.rights.help"), tokens));
    for class in POLICY_CLASSES {
        let current = rule_of(&draft.policy, class);
        let choices = rules_for(class).iter().map(|&rule| {
            iced::widget::radio(rule_title(rule), rule, Some(current), move |rule| {
                wrap(SettingsMessage::Rule(class, rule))
            })
            .into()
        });
        // A class's rules are one stop of the ring; the arrows choose (RC-080-16).
        let group = focus::pick(
            Row::with_children(choices)
                .spacing(16)
                .align_y(Alignment::Center),
            class_title(class),
            rules_for(class),
            Some(&current),
            move |rule| wrap(SettingsMessage::Rule(class, rule)),
        );
        rules = rules.push(
            row![label(class_title(class)).width(120), group]
                .spacing(16)
                .align_y(Alignment::Center),
        );
    }
    let token = Column::new()
        .spacing(8)
        .push(label(t!("settings.mcp.token")))
        .push(hint(t!("settings.mcp.token.help"), tokens))
        .push(
            row![
                token_button(
                    Some(Icon::Copy),
                    t!("settings.mcp.copy_token"),
                    SettingsMessage::CopyToken
                ),
                token_button(
                    None,
                    t!("settings.mcp.rotate_token"),
                    SettingsMessage::RotateToken
                ),
            ]
            .spacing(8),
        );
    Column::new()
        .spacing(16)
        .push(rules)
        .push(token)
        .push(hint(t!("settings.mcp.protected_note"), tokens))
        .into()
}
