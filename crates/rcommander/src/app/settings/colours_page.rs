//! The colours by file type (RC-100-43, after Total Commander's "Define colors by file type"),
//! drawn on the Themes tab under the themes (RC-100-50): one rule per row - mask, light and dark
//! colour, moved up and down, removed - with Add rule below them. The rules stay global, apart
//! from the themes; what is stored is applied with the themes (`themes_page::read`). On Windows
//! every colour also opens the system's colour dialog (`platform::colour_picker`), so nobody has
//! to type `#rrggbb`.

use iced::{
    Background, Border, Color, Element, Task,
    widget::{Space, container},
};
use rc_engine::settings::COLOUR_RULES_KEY;

mod rule_list;

pub use self::rule_list::RuleOp;
pub(super) use self::rule_list::rules;
use self::rule_list::{edit_rule, typed_rules, with_rule_colour};
use super::{
    SettingsMessage,
    draft::{Draft, Field},
    wrap,
};
use crate::{
    app::{App, Effect, Message},
    platform::colour_picker,
    theme::{Tokens, colours::parse_hex, layout::secondary_button},
};

/// A colour the system's dialog is opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickTarget {
    /// Rule `index`'s light colour, or its dark one.
    Rule { index: usize, dark: bool },
}

/// The colour `target` draws now, as the dialog opens on it.
fn current(draft: &Draft, target: PickTarget) -> Option<Color> {
    let PickTarget::Rule { index, dark } = target;
    let rules = typed_rules(text_of(draft, COLOUR_RULES_KEY));
    let (_, light, dark_colour) = rules.get(index)?;
    let light = parse_hex(light);
    if dark {
        parse_hex(dark_colour).or(light)
    } else {
        light
    }
}

/// Opens the system's colour dialog on what `target` draws now, off the UI thread; its answer
/// comes back as [`SettingsMessage::Picked`].
pub(super) fn pick(draft: &Draft, target: PickTarget) -> Task<Message> {
    let initial = current(draft, target).map(|colour| {
        let [red, green, blue, _] = colour.into_rgba8();
        [red, green, blue]
    });
    colour_picker::pick(initial).map(move |chosen| {
        let hex = chosen.map(hex_of);
        wrap(SettingsMessage::Picked(target, hex))
    })
}

impl App {
    /// The rules' own messages: a rule edited, the dialog opened and answered; any other message
    /// back.
    pub(super) fn colours_message(
        &mut self,
        message: SettingsMessage,
    ) -> Result<Vec<Effect>, SettingsMessage> {
        let draft = self
            .settings
            .as_ref()
            .and_then(|dialog| dialog.draft.as_ref());
        match message {
            SettingsMessage::Pick(target) => Ok(draft
                .map(|draft| Effect::Task(pick(draft, target)))
                .into_iter()
                .collect()),
            SettingsMessage::ColourRule(index, op) => match draft {
                Some(draft) => {
                    let text = edit_rule(text_of(draft, COLOUR_RULES_KEY), index, op);
                    Ok(self.settings_message(SettingsMessage::Edit(COLOUR_RULES_KEY, text)))
                }
                None => Ok(Vec::new()),
            },
            SettingsMessage::Picked(target, hex) => {
                match draft
                    .zip(hex)
                    .map(|(draft, hex)| picked(draft, target, hex))
                {
                    Some(edit) => Ok(self.settings_message(edit)),
                    None => Ok(Vec::new()),
                }
            }
            other => Err(other),
        }
    }
}

/// The edit the colour `hex` chosen for `target` makes: the rule list with the rule's colour
/// replaced.
pub(super) fn picked(draft: &Draft, target: PickTarget, hex: String) -> SettingsMessage {
    let PickTarget::Rule { index, dark } = target;
    SettingsMessage::Edit(
        COLOUR_RULES_KEY,
        with_rule_colour(text_of(draft, COLOUR_RULES_KEY), index, dark, hex),
    )
}

/// A colour of the dialog as `#rrggbb`.
fn hex_of([red, green, blue]: [u8; 3]) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

/// The button that opens the dialog for `target`, on a platform that has it.
fn choose<'a>(label: &'a str, target: PickTarget) -> Option<Element<'a, Message>> {
    colour_picker::AVAILABLE
        .then(|| secondary_button(label, Some(wrap(SettingsMessage::Pick(target)))))
}

fn text_of<'a>(draft: &'a Draft, key: &str) -> &'a str {
    match draft.field(key) {
        Some(Field::Text(text)) => text.as_str(),
        _ => "",
    }
}

/// A small square of `colour`, framed so white and black show on either look.
fn swatch<'a>(colour: Color, tokens: &'static Tokens) -> Element<'a, Message> {
    container(Space::new().width(20).height(20))
        .style(move |_| container::Style {
            background: Some(Background::Color(colour)),
            border: Border::default()
                .rounded(3.0)
                .width(1.0)
                .color(tokens.stroke),
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
#[path = "colours_page_tests.rs"]
mod tests;
