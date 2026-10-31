//! Drawing the theme editor (RC-100-50): the name, the accent, each group of tokens with a line
//! per colour - its label, swatch, Choose..., `#rrggbb` and Reset - the terminal's preview, the
//! warnings for the pairs below 4.5:1, then Save and Cancel. The swatches, the preview and the
//! warnings show the look of the last pause (`editor.rs`), the fields what is typed.

use iced::{
    Alignment, Background, Element, Length,
    widget::{Column, checkbox, column, container, row, text_input},
};
use rc_engine::settings::themes::ThemeAccent;

use super::{
    ThemesMsg,
    editor::{Editor, EditorMsg, Slot},
    labels::{GROUPS, Group, group_title, token_label},
    page_message,
    view::swatch,
};
use crate::{
    app::{
        Message,
        settings::view::{hint, label},
    },
    i18n::t,
    platform::colour_picker,
    theme::{
        DANGER, MONO_FONT, Tokens,
        colours::low_contrast,
        fit::line,
        focus,
        icon::Icon,
        layout::{primary_icon_button, secondary_button, secondary_icon_button},
    },
};

fn edit(message: EditorMsg) -> Message {
    page_message(ThemesMsg::Editor(message))
}

/// One colour of the theme: its label, its swatch, Choose... where the system has the dialog, its
/// `#rrggbb` and - for a token - Reset to the value the theme opened with.
fn colour_line<'a>(
    editor: &'a Editor,
    slot: Slot,
    title: &'a str,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let typed = editor.typed.get(&slot).map_or("", String::as_str);
    let shown = editor.colour(slot).unwrap_or(tokens.text);
    let mut line = row![label(title).width(260), swatch(shown, 20.0, tokens)]
        .spacing(8)
        .align_y(Alignment::Center);
    if colour_picker::AVAILABLE {
        line = line.push(secondary_button(
            t!("settings.colours.choose"),
            Some(edit(EditorMsg::Pick(slot))),
        ));
    }
    line = line.push(
        text_input(t!("settings.colours.own"), typed)
            .on_input(move |text| edit(EditorMsg::Hex(slot, text)))
            .width(110),
    );
    if let Slot::Token(key) = slot {
        line = line.push(secondary_button(
            t!("settings.colours.reset"),
            editor.changed(key).then(|| edit(EditorMsg::Reset(key))),
        ));
    }
    line.into()
}

/// A prompt, a listing in its colours and the sixteen colours, on the terminal's background.
fn terminal_preview<'a>(editor: &Editor) -> Element<'a, Message> {
    let terminal = editor.shown.terminal;
    let mono = |text: &'static str, colour| line(text).font(MONO_FONT).color(colour);
    let listing = row![
        mono("docs/", terminal.blue),
        mono("run.sh", terminal.green),
        mono("notes.txt", terminal.foreground),
        mono("backup.zip", terminal.red),
        mono("link", terminal.cyan),
    ]
    .spacing(16);
    let normal = [
        terminal.black,
        terminal.red,
        terminal.green,
        terminal.yellow,
        terminal.blue,
        terminal.magenta,
        terminal.cyan,
        terminal.white,
    ];
    let bright = [
        terminal.bright_black,
        terminal.bright_red,
        terminal.bright_green,
        terminal.bright_yellow,
        terminal.bright_blue,
        terminal.bright_magenta,
        terminal.bright_cyan,
        terminal.bright_white,
    ];
    let shades = |colours: [iced::Color; 8]| {
        let mut strip = row![].spacing(4);
        for colour in colours {
            strip = strip.push(swatch(colour, 16.0, editor.shown));
        }
        strip
    };
    let background = terminal.background;
    container(
        column![
            mono(t!("settings.themes.sample_prompt"), terminal.green),
            listing,
            shades(normal),
            shades(bright),
        ]
        .spacing(6),
    )
    .padding(10)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(background)),
        ..container::Style::default()
    })
    .into()
}

/// The warning for the pair `theme::colours::low_contrast` names.
fn contrast_note(pair: &str) -> &'static str {
    match pair {
        "settings.colours.low_text" => t!("settings.colours.low_text"),
        "settings.colours.low_stripe" => t!("settings.colours.low_stripe"),
        "settings.colours.low_cursor" => t!("settings.colours.low_cursor"),
        "settings.colours.low_surface" => t!("settings.colours.low_surface"),
        "settings.colours.low_terminal" => t!("settings.colours.low_terminal"),
        _ => t!("settings.colours.low_marked"),
    }
}

/// The editor in the place of the tab.
pub fn view<'a>(editor: &'a Editor, busy: bool, tokens: &'static Tokens) -> Element<'a, Message> {
    let mode = if editor.theme.dark {
        t!("settings.themes.dark")
    } else {
        t!("settings.themes.light")
    };
    let system = editor.theme.accent == ThemeAccent::System;
    let mut body = Column::new().spacing(8).push(
        row![
            label(t!("settings.themes.name")),
            text_input(t!("settings.themes.name"), &editor.theme.name)
                .on_input(|text| edit(EditorMsg::Name(text)))
                .width(280),
            hint(mode, tokens),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    for (group, keys) in GROUPS {
        body = body.push(label(group_title(group)).size(14));
        if group == Group::Accent {
            let accent = checkbox(system)
                .label(t!("settings.themes.system_accent"))
                .on_toggle(|on| edit(EditorMsg::SystemAccent(on)));
            body = body.push(focus::toggle(
                accent,
                "settings.themes.system_accent",
                Some(edit(EditorMsg::SystemAccent(!system))),
            ));
            if !system {
                body = body.push(colour_line(
                    editor,
                    Slot::Accent,
                    t!("settings.themes.accent"),
                    tokens,
                ));
            }
        }
        for &key in keys {
            body = body.push(colour_line(
                editor,
                Slot::Token(key),
                token_label(key),
                tokens,
            ));
        }
        if group == Group::Terminal {
            body = body.push(terminal_preview(editor));
        }
    }
    for pair in low_contrast(editor.shown) {
        body = body.push(line(contrast_note(pair)).color(DANGER));
    }
    body.push(hint(t!("settings.themes.editor_help"), tokens))
        .push(
            row![
                primary_icon_button(
                    Icon::Save,
                    t!("settings.themes.save"),
                    (!busy).then(|| edit(EditorMsg::Save))
                ),
                secondary_icon_button(
                    Icon::Close,
                    t!("settings.themes.cancel"),
                    Some(edit(EditorMsg::Cancel))
                ),
            ]
            .spacing(8),
        )
        .into()
}
