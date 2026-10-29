//! The rules by file type on the Colours tab: the rule list as typed, a row each, and the edits of
//! one row as a message naming the row and what changes ([`RuleOp`], RC-080-53) - the view clones
//! no rule list for its rows.

use iced::{
    Alignment, Color, Element, Length,
    widget::{Column, row, text_input},
};
use rc_engine::settings::COLOUR_RULES_KEY;

use super::{PickTarget, choose, swatch, text_of};
use crate::{
    app::{
        Message,
        settings::{SettingsMessage, draft::Draft, view::hint, wrap},
    },
    i18n::t,
    theme::{
        Tokens,
        colours::parse_hex,
        icon::Icon,
        layout::{secondary_button, secondary_icon_button},
    },
};

/// One rule as typed: mask, light colour, dark colour (empty: the light one).
pub(super) type Typed = (String, String, String);

/// What an edit does to one rule of the list, or to the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleOp {
    Mask(String),
    Light(String),
    Dark(String),
    Up,
    Down,
    Remove,
    /// A new rule at the end; the row it names does not matter.
    Add,
}

/// The rules the draft holds, as typed: a line each, its fields split at tabs. A blank line is
/// no rule - the engine reads the list the same way (`parse_colour_rules`), so the rows shown
/// are the rules stored.
pub(super) fn typed_rules(text: &str) -> Vec<Typed> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t').map(str::to_owned);
            (
                fields.next().unwrap_or_default(),
                fields.next().unwrap_or_default(),
                fields.next().unwrap_or_default(),
            )
        })
        .collect()
}

/// `rules` written back as the rule list, a line each.
pub(super) fn written(rules: &[Typed]) -> String {
    rules
        .iter()
        .map(|(mask, light, dark)| {
            if dark.is_empty() {
                format!("{mask}\t{light}")
            } else {
                format!("{mask}\t{light}\t{dark}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The rule list `text` after `op` on rule `index`; a row that is gone changes nothing.
pub(super) fn edit_rule(text: &str, index: usize, op: RuleOp) -> String {
    let mut rules = typed_rules(text);
    let last = rules.len().saturating_sub(1);
    match op {
        RuleOp::Add => rules.push(("*.exe".to_owned(), "#c42b1c".to_owned(), String::new())),
        _ if index >= rules.len() => {}
        RuleOp::Mask(typed) => rules[index].0 = typed,
        RuleOp::Light(typed) => rules[index].1 = typed,
        RuleOp::Dark(typed) => rules[index].2 = typed,
        RuleOp::Up if index > 0 => rules.swap(index, index - 1),
        RuleOp::Down if index < last => rules.swap(index, index + 1),
        RuleOp::Up | RuleOp::Down => {}
        RuleOp::Remove => {
            rules.remove(index);
        }
    }
    written(&rules)
}

/// The rule list `text` with rule `index`'s light or dark colour set to `hex`.
pub(super) fn with_rule_colour(text: &str, index: usize, dark: bool, hex: String) -> String {
    let op = if dark {
        RuleOp::Dark(hex)
    } else {
        RuleOp::Light(hex)
    };
    edit_rule(text, index, op)
}

fn edit(index: usize, op: RuleOp) -> Message {
    wrap(SettingsMessage::ColourRule(index, op))
}

/// The rules by file type: a row each - the mask, the light and the dark colour with their
/// swatches, Up, Down, Remove - then Add rule. The first rule matching a name colours it.
pub(in crate::app::settings) fn rules<'a>(
    draft: &'a Draft,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let typed = typed_rules(text_of(draft, COLOUR_RULES_KEY));
    let last = typed.len().saturating_sub(1);
    let mut list = Column::new().spacing(6);
    for (index, (mask, light, dark)) in typed.iter().enumerate() {
        let mask_field = text_input(t!("settings.colours.mask"), mask)
            .on_input(move |text| edit(index, RuleOp::Mask(text)))
            .width(Length::Fill);
        let light_field = text_input("#rrggbb", light)
            .on_input(move |text| edit(index, RuleOp::Light(text)))
            .width(90);
        let dark_field = text_input(t!("settings.colours.dark_same"), dark)
            .on_input(move |text| edit(index, RuleOp::Dark(text)))
            .width(90);
        let light_colour = parse_hex(light).unwrap_or(Color::TRANSPARENT);
        let dark_colour = parse_hex(dark).unwrap_or(light_colour);
        let up = (index > 0).then(|| edit(index, RuleOp::Up));
        let down = (index < last).then(|| edit(index, RuleOp::Down));
        list = list.push(
            row![mask_field, swatch(light_colour, tokens)]
                .push(choose(
                    t!("settings.colours.choose_short"),
                    PickTarget::Rule { index, dark: false },
                ))
                .push(light_field)
                .push(swatch(dark_colour, tokens))
                .push(choose(
                    t!("settings.colours.choose_short"),
                    PickTarget::Rule { index, dark: true },
                ))
                .push(dark_field)
                .push(secondary_button(t!("settings.colours.up"), up))
                .push(secondary_button(t!("settings.colours.down"), down))
                .push(secondary_icon_button(
                    Icon::Delete,
                    t!("settings.colours.remove"),
                    Some(edit(index, RuleOp::Remove)),
                ))
                .spacing(6)
                .align_y(Alignment::Center),
        );
    }
    list = list.push(secondary_icon_button(
        Icon::Add,
        t!("settings.colours.add_rule"),
        Some(edit(typed.len(), RuleOp::Add)),
    ));
    if typed.is_empty() {
        list = list.push(hint(t!("settings.colours.no_rules"), tokens));
    }
    list.into()
}
