//! The Colours tab of the Appearance page (RC-100-43, after Total Commander's Configuration,
//! Color): each colour of the light and the dark look as a swatch, its `#rrggbb` and a Reset; a
//! preview of the list in the colours being edited, with a warning for every pair below WCAG
//! AA's contrast; and the rules by file type, one row each - mask, light and dark colour, moved up
//! and down, removed - with Add rule below them.
//!
//! What is stored is applied to the theme at the start and on every change ([`read`], [`apply`]).
//! On Windows every colour also opens the system's colour dialog (Choose..., and ... at a rule),
//! so nobody has to type `#rrggbb` (`platform::colour_picker`).

use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Column, Space, column, container, row, text_input},
};
use rc_db::Db;
use rc_engine::settings::{
    COLOUR_KEYS, COLOUR_RULES_KEY, COLOUR_TOKENS, SettingSpec, colour_key, stored_setting,
};

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
    i18n::t,
    platform::colour_picker,
    theme::{
        self, Tokens,
        colour_rules::{compile, first_match},
        colours::{Colours, build, low_contrast, parse_hex},
        fit::line,
        layout::secondary_button,
    },
};

/// The colours of both looks and the rule list, as stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColourSettings {
    /// The light look's colours, one per `COLOUR_TOKENS`.
    pub light: Vec<String>,
    pub dark: Vec<String>,
    pub rules: String,
}

/// Reads the colours and the rules.
pub async fn read(db: Db) -> ColourSettings {
    let mut values = Vec::with_capacity(COLOUR_KEYS.len());
    for key in COLOUR_KEYS {
        let stored = stored_setting(&db, key).await;
        values.push(
            stored
                .as_ref()
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
        );
    }
    let rules = values.pop().unwrap_or_default();
    let dark = values.split_off(COLOUR_TOKENS.len());
    ColourSettings {
        light: values,
        dark,
        rules,
    }
}

/// The colours and the rules in force from now on.
pub fn apply(settings: &ColourSettings) {
    theme::colours::set_colours(
        [
            Colours::from_values(&settings.light),
            Colours::from_values(&settings.dark),
        ],
        theme::system_accent(),
    );
    theme::colour_rules::set_rules(&settings.rules);
}

/// A colour the system's dialog is opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickTarget {
    /// The colour setting `key`.
    Setting(&'static str),
    /// Rule `index`'s light colour, or its dark one.
    Rule { index: usize, dark: bool },
}

/// The colour `target` draws now, as the dialog opens on it.
fn current(draft: &Draft, target: PickTarget) -> Option<Color> {
    match target {
        PickTarget::Setting(key) => {
            let dark = key.starts_with("appearance.colours.dark.");
            let token = key.rsplit('.').next().unwrap_or_default();
            let look = drafted_look(draft, dark);
            Some(parse_hex(text_of(draft, key)).unwrap_or_else(|| token_colour(&look, token)))
        }
        PickTarget::Rule { index, dark } => {
            let rules = typed_rules(text_of(draft, COLOUR_RULES_KEY));
            let (_, light, dark_colour) = rules.get(index)?;
            let light = parse_hex(light);
            if dark {
                parse_hex(dark_colour).or(light)
            } else {
                light
            }
        }
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
    /// The Colours tab's own messages: the stored colours read, a rule edited, the dialog opened
    /// and answered; any other message back.
    pub(super) fn colours_message(
        &mut self,
        message: SettingsMessage,
    ) -> Result<Vec<Effect>, SettingsMessage> {
        let draft = self
            .settings
            .as_ref()
            .and_then(|dialog| dialog.draft.as_ref());
        match message {
            SettingsMessage::Colours(colours) => {
                apply(&colours);
                Ok(Vec::new())
            }
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

/// The edit the colour `hex` chosen for `target` makes: the setting's text, or the rule list with
/// the rule's colour replaced.
pub(super) fn picked(draft: &Draft, target: PickTarget, hex: String) -> SettingsMessage {
    match target {
        PickTarget::Setting(key) => SettingsMessage::Edit(key, hex),
        PickTarget::Rule { index, dark } => SettingsMessage::Edit(
            COLOUR_RULES_KEY,
            with_rule_colour(text_of(draft, COLOUR_RULES_KEY), index, dark, hex),
        ),
    }
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

/// Whether the setting `key` is one of the colours or the rules.
pub fn concerns(key: &str) -> bool {
    COLOUR_KEYS.contains(&key)
}

fn text_of<'a>(draft: &'a Draft, key: &str) -> &'a str {
    match draft.field(key) {
        Some(Field::Text(text)) => text.as_str(),
        _ => "",
    }
}

/// The colours of the look `dark` as the draft holds them.
fn drafted(draft: &Draft, dark: bool) -> Colours {
    let values: Vec<String> = COLOUR_TOKENS
        .iter()
        .map(|token| {
            colour_key(dark, token)
                .map(|key| text_of(draft, key).to_owned())
                .unwrap_or_default()
        })
        .collect();
    Colours::from_values(&values)
}

/// The look `dark` as the draft would draw it.
fn drafted_look(draft: &Draft, dark: bool) -> Tokens {
    build(
        dark,
        theme::system_accent()[usize::from(dark)],
        &drafted(draft, dark),
    )
}

/// The colour `token` of `look`.
fn token_colour(look: &Tokens, token: &str) -> Color {
    match token {
        "text" => look.list_text,
        "background" => look.list_fill,
        "stripe" => theme::colours::over(look.stripe_fill, look.list_fill),
        "cursor" => theme::colours::over(look.cursor_fill, look.list_fill),
        "cursor_inactive" => theme::colours::over(look.cursor_fill_inactive, look.list_fill),
        "cursor_text" => look.cursor_text,
        "marked" => look.marked,
        _ => look.accent,
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

/// The control of one colour setting: the swatch of what it draws now, the `#rrggbb` (empty for
/// the look's own) and Reset; under the accent, the last of a look, the look's preview.
pub(super) fn control<'a>(
    spec: &SettingSpec,
    draft: &'a Draft,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let key = spec.key;
    let dark = key.starts_with("appearance.colours.dark.");
    let token = key.rsplit('.').next().unwrap_or_default();
    let look = drafted_look(draft, dark);
    let typed = text_of(draft, key);
    let shown = parse_hex(typed).unwrap_or_else(|| token_colour(&look, token));
    let field = text_input(t!("settings.colours.own"), typed)
        .on_input(move |text| wrap(SettingsMessage::Edit(key, text)))
        .width(110);
    let reset = secondary_button(
        t!("settings.colours.reset"),
        (!typed.is_empty()).then(|| wrap(SettingsMessage::Edit(key, String::new()))),
    );
    let line = row![swatch(shown, tokens)]
        .push(choose(
            t!("settings.colours.choose"),
            PickTarget::Setting(key),
        ))
        .push(field)
        .push(reset)
        .spacing(8)
        .align_y(Alignment::Center);
    if token != "accent" {
        return line.into();
    }
    column![line, preview(draft, &look, tokens)]
        .spacing(8)
        .into()
}

/// A few rows as the lists will draw them in `look`, and a warning per pair too weak to read.
fn preview<'a>(draft: &'a Draft, look: &Tokens, tokens: &'static Tokens) -> Element<'a, Message> {
    let rules = compile(text_of(draft, COLOUR_RULES_KEY));
    // (name, folder, cursor, marked, alternate)
    let samples = [
        ("Documents", true, false, false, false),
        ("report.pdf", false, false, false, true),
        ("photo.jpg", false, true, false, false),
        ("notes.txt", false, false, true, true),
        ("setup.exe", false, false, false, false),
    ];
    let mut rows = Column::new().spacing(1);
    for (name, dir, cursor, marked, alternate) in samples {
        let style = theme::striped_row(look, cursor, true, marked, alternate);
        let colour = if marked {
            look.marked
        } else if let Some(rule) = first_match(&rules, name, dir, look.dark) {
            rule
        } else if cursor {
            look.cursor_text
        } else {
            look.list_text
        };
        let label = if dir {
            format!("[{name}]")
        } else {
            name.to_owned()
        };
        let secondary = if marked {
            look.marked
        } else {
            look.list_text_secondary
        };
        rows = rows.push(
            container(
                row![
                    line(label).color(colour).width(Length::Fill),
                    line(t!("settings.colours.sample_size")).color(secondary),
                ]
                .padding([2, 8]),
            )
            .width(Length::Fill)
            .style(move |_| style),
        );
    }
    let fill = look.list_fill;
    let stroke = tokens.stroke;
    let mut block = Column::new()
        .spacing(4)
        .push(
            container(rows)
                .width(360)
                .padding(4)
                .style(move |_| container::Style {
                    background: Some(Background::Color(fill)),
                    border: Border::default().rounded(4.0).width(1.0).color(stroke),
                    ..container::Style::default()
                }),
        );
    for warning in low_contrast(look) {
        block = block.push(line(contrast_note(warning)).color(theme::DANGER));
    }
    block.into()
}

/// The warning for the pair `theme::colours::low_contrast` names.
fn contrast_note(pair: &str) -> &'static str {
    match pair {
        "settings.colours.low_text" => t!("settings.colours.low_text"),
        "settings.colours.low_stripe" => t!("settings.colours.low_stripe"),
        "settings.colours.low_cursor" => t!("settings.colours.low_cursor"),
        _ => t!("settings.colours.low_marked"),
    }
}

#[cfg(test)]
#[path = "colours_page_tests.rs"]
mod tests;
