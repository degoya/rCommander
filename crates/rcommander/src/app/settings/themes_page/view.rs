//! Drawing the Themes tab (RC-100-50): the theme of each mode, the list of themes with a tile of
//! each one's colours, the buttons, the file, the question of an import's name and the last note,
//! then the colours by file type. While a theme is edited the editor takes the tab
//! (`editor_view.rs`). Every control is a stop of the dialog's focus ring.

use std::fmt;

use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{Column, Space, button, column, container, pick_list, row, text_input},
};
use rc_engine::settings::{
    THEME_DARK_KEY, THEME_LIGHT_KEY,
    themes::{Theme, ThemesState},
};

use super::{
    ConflictChoice, ThemesMsg, ThemesPage, actions::shown_name, editor_view, page_message,
};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
        settings::{
            SettingsMessage,
            draft::{Draft, Field},
            view::{hint, label},
            wrap,
        },
    },
    i18n::{lookup, t},
    theme::{
        self, DANGER, Tokens, focus,
        icon::Icon,
        layout::{primary_button, secondary_button, secondary_icon_button},
        themes::seen,
    },
};

/// A theme in a mode's pick list: its id, shown by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Choice {
    id: String,
    name: String,
}

impl fmt::Display for Choice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

fn choice(theme: &Theme) -> Choice {
    Choice {
        id: theme.id.clone(),
        name: shown_name(theme).to_owned(),
    }
}

/// The theme the draft names for `key`; the one in force where it names none.
fn drafted<'a>(draft: &'a Draft, key: &str, state: &'a ThemesState, dark: bool) -> &'a str {
    match draft.field(key) {
        Some(Field::Text(text)) if state.theme(text).is_some() => text.as_str(),
        _ => state.chosen[usize::from(dark)].as_str(),
    }
}

/// The theme of the light or the dark mode: a pick list of that mode's themes.
fn mode_pick<'a>(
    state: &'a ThemesState,
    draft: &'a Draft,
    dark: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let key = if dark {
        THEME_DARK_KEY
    } else {
        THEME_LIGHT_KEY
    };
    let options: Vec<Choice> = state
        .themes
        .iter()
        .filter(|theme| theme.dark == dark)
        .map(choice)
        .collect();
    let chosen = drafted(draft, key, state, dark);
    let selected = options.iter().find(|option| option.id == chosen).cloned();
    let edit = move |option: Choice| wrap(SettingsMessage::Edit(key, option.id));
    let moves = focus::moves(&options, selected.as_ref(), &edit);
    let list = pick_list(options, selected, edit).width(240);
    let (title, help) = if dark {
        (
            t!("setting.appearance.theme_dark"),
            t!("setting.appearance.theme_dark.help"),
        )
    } else {
        (
            t!("setting.appearance.theme_light"),
            t!("setting.appearance.theme_light.help"),
        )
    };
    column![
        label(title),
        focus::Stop::pick(list, key, moves),
        hint(help, tokens),
    ]
    .spacing(4)
    .into()
}

/// A small square of `colour`, framed so white and black show on either look.
pub(super) fn swatch<'a>(
    colour: Color,
    size: f32,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    container(Space::new().width(size).height(size))
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

/// A theme's tile: its window, lists, accent, marks and terminal side by side.
fn tile<'a>(theme: &Theme, tokens: &'static Tokens) -> Element<'a, Message> {
    let look = theme::themes::look(theme);
    let colours = [
        look.window,
        look.list_fill,
        seen(look, "cursor_fill").unwrap_or(look.accent),
        look.accent,
        look.marked,
        look.terminal.background,
    ];
    let mut strip = row![].spacing(2);
    for colour in colours {
        strip = strip.push(swatch(colour, 12.0, tokens));
    }
    strip.into()
}

/// The list of every theme: its tile, its name, its mode and whether it is built in; the arrows
/// mark the one before and after.
fn list<'a>(
    page: &'a ThemesPage,
    state: &'a ThemesState,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let ids: Vec<String> = state.themes.iter().map(|theme| theme.id.clone()).collect();
    let mut rows = Column::new().spacing(2);
    for theme in &state.themes {
        let marked = page.selected.as_deref() == Some(theme.id.as_str());
        let mode = if theme.dark {
            t!("settings.themes.dark")
        } else {
            t!("settings.themes.light")
        };
        let kind = if theme.builtin {
            t!("settings.themes.builtin")
        } else {
            t!("settings.themes.own")
        };
        let line = row![
            tile(theme, tokens),
            label(shown_name(theme)).width(Length::Fill),
            hint(mode, tokens).width(60),
            hint(kind, tokens).width(80),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        rows = rows.push(
            button(line)
                .width(Length::Fill)
                .padding([4, 8])
                .style(move |_, status| theme::page_entry(tokens, marked, status))
                .on_press(page_message(ThemesMsg::Choose(theme.id.clone()))),
        );
    }
    focus::pick(
        rows.width(Length::Fill),
        "settings.themes.list",
        &ids,
        page.selected.as_ref(),
        |id| page_message(ThemesMsg::Choose(id)),
    )
}

/// New, Duplicate, Rename, Edit and Delete - or the new name's field while one is renamed.
fn buttons<'a>(page: &'a ThemesPage, tokens: &'static Tokens) -> Element<'a, Message> {
    let ready = !page.busy;
    let on = |enabled: bool, message: ThemesMsg| (ready && enabled).then(|| page_message(message));
    let marked = page.marked();
    let own = marked.is_some_and(|theme| !theme.builtin);
    let in_use = marked.is_some_and(|theme| page.chosen(&theme.id));
    if let Some(name) = &page.renaming {
        return row![
            text_input(t!("settings.themes.name"), name)
                .on_input(|text| page_message(ThemesMsg::RenameText(text)))
                .on_submit(page_message(ThemesMsg::RenameDone))
                .width(280),
            primary_button(
                t!("settings.themes.rename_done"),
                on(true, ThemesMsg::RenameDone)
            ),
            secondary_button(
                t!("settings.themes.rename_cancel"),
                Some(page_message(ThemesMsg::RenameCancel))
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into();
    }
    let mut block = Column::new().spacing(4).push(
        row![
            secondary_icon_button(
                Icon::Add,
                t!("settings.themes.new"),
                on(true, ThemesMsg::New)
            ),
            secondary_icon_button(
                Icon::Copy,
                t!("settings.themes.duplicate"),
                on(marked.is_some(), ThemesMsg::Duplicate)
            ),
            secondary_icon_button(
                Icon::Rename,
                t!("settings.themes.rename"),
                on(own, ThemesMsg::Rename)
            ),
            secondary_button(t!("settings.themes.edit"), on(own, ThemesMsg::Edit)),
            secondary_icon_button(
                Icon::Delete,
                t!("settings.themes.delete"),
                on(own && !in_use, ThemesMsg::Delete)
            ),
        ]
        .spacing(8),
    );
    if own && in_use {
        block = block.push(hint(t!("settings.themes.in_use"), tokens));
    }
    if marked.is_some_and(|theme| theme.builtin) {
        block = block.push(hint(t!("settings.themes.builtin_hint"), tokens));
    }
    block.into()
}

/// The file Import reads and Export writes, with Browse... where the system has a dialog.
fn file_row<'a>(page: &'a ThemesPage, tokens: &'static Tokens) -> Element<'a, Message> {
    let ready = !page.busy;
    let on = |message: ThemesMsg| ready.then(|| page_message(message));
    let export = page.marked().is_some().then_some(ThemesMsg::Export);
    column![
        label(t!("settings.themes.file")),
        row![
            text_input(t!("settings.themes.file_placeholder"), &page.file)
                .on_input(|text| page_message(ThemesMsg::File(text)))
                .width(Length::Fill),
        ]
        .push(browse::button(PathField::ThemeFile, &page.file, ready))
        .push(secondary_icon_button(
            Icon::Import,
            t!("settings.themes.import"),
            on(ThemesMsg::Import)
        ))
        .push(secondary_button(
            t!("settings.themes.export"),
            export.and_then(on)
        ))
        .spacing(6)
        .align_y(Alignment::Center),
        hint(t!("settings.themes.file_help"), tokens),
    ]
    .spacing(4)
    .into()
}

/// The question when an import's name is taken: replace that theme, take another name, or stop.
fn conflict(page: &ThemesPage) -> Option<Element<'_, Message>> {
    let conflict = page.conflict.as_ref()?;
    let ask = |choice| Some(page_message(ThemesMsg::Conflict(choice)));
    Some(
        column![
            label(t!(
                "settings.themes.conflict",
                name = conflict.existing.name.as_str()
            )),
            row![
                secondary_button(
                    t!("settings.themes.conflict_replace"),
                    ask(ConflictChoice::Replace)
                ),
                secondary_button(
                    t!("settings.themes.conflict_rename"),
                    ask(ConflictChoice::Rename)
                ),
                secondary_button(
                    t!("settings.themes.conflict_cancel"),
                    ask(ConflictChoice::Cancel)
                ),
            ]
            .spacing(8),
        ]
        .spacing(6)
        .into(),
    )
}

/// The Themes tab: the editor while a theme is edited, else the modes, the list, the buttons, the
/// file, the question, the note and the colours by file type.
pub fn view<'a>(
    page: &'a ThemesPage,
    draft: &'a Draft,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    if let Some(editor) = &page.editor {
        return editor_view::view(editor, page.busy, tokens);
    }
    let Some(state) = &page.state else {
        return label(t!("settings.loading")).into();
    };
    let mut body = Column::new()
        .spacing(16)
        .push(
            row![
                mode_pick(state, draft, false, tokens),
                mode_pick(state, draft, true, tokens)
            ]
            .spacing(24),
        )
        .push(
            column![
                label(t!("settings.themes.list")),
                list(page, state, tokens),
                buttons(page, tokens),
            ]
            .spacing(6),
        )
        .push(file_row(page, tokens))
        .push(conflict(page));
    if let Some((note, error)) = &page.note {
        let colour = if *error {
            DANGER
        } else {
            tokens.text_secondary
        };
        body = body.push(label(note.as_str()).color(colour));
    }
    body.push(
        column![
            label(lookup("setting.colours.rules")),
            super::super::colours_page::rules(draft, tokens),
            hint(lookup("setting.colours.rules.help"), tokens),
        ]
        .spacing(4),
    )
    .into()
}
